// Progress of long jobs (installs, downloads), fed by the `job` events from Rust.
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";

export interface JobEvent {
    job: string;
    state: "running" | "done" | "error";
    stage: string;
    done: number;
    total: number;
    line: string | null;
    error: string | null;
}

export interface JobState {
    state: "running" | "done" | "error";
    stage: string;
    done: number;
    total: number;
    lines: string[];
    error: string | null;
}

type Listener = (jobs: Record<string, JobState>) => void;

let jobs: Record<string, JobState> = {};
const listeners = new Set<Listener>();
let started = false;

function publish() {
    for (const l of listeners) l(jobs);
}

function start() {
    if (started) return;
    started = true;
    api.runningJobs()
        .then((ids) => {
            for (const id of ids) {
                jobs = { ...jobs, [id]: jobs[id] ?? { state: "running", stage: "", done: 0, total: 0, lines: [], error: null } };
            }
            publish();
        })
        .catch(() => {});
    listen<JobEvent>("job", ({ payload: e }) => {
        const prev = jobs[e.job];
        const base: JobState =
            prev && prev.state === "running" ? prev : { state: "running", stage: "", done: 0, total: 0, lines: [], error: null };
        const next: JobState = { ...base, state: e.state, error: e.error };
        if (e.line) next.lines = [...base.lines.slice(-200), e.line];
        if (e.total > 0) {
            next.done = e.done;
            next.total = e.total;
            next.stage = e.stage;
        }
        jobs = { ...jobs, [e.job]: next };
        publish();
    });
}

/** All jobs this session has seen, keyed by job id. */
export function useJobs(): Record<string, JobState> {
    const [state, setState] = useState(jobs);
    useEffect(() => {
        start();
        listeners.add(setState);
        return () => {
            listeners.delete(setState);
        };
    }, []);
    return state;
}

/** Wait until a job ends; resolves on done and rejects with its error. */
export function waitForJob(id: string): Promise<void> {
    return new Promise((resolve, reject) => {
        start();
        const check: Listener = (all) => {
            const j = all[id];
            if (!j || j.state === "running") return;
            listeners.delete(check);
            if (j.state === "done") resolve();
            else reject(j.error ?? "failed");
        };
        listeners.add(check);
    });
}

export function clearJob(id: string) {
    const { [id]: _gone, ...rest } = jobs;
    jobs = rest;
    publish();
}
