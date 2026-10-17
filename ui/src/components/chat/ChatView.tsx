import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ArrowDown } from "lucide-react";
import { useApp } from "@/store/app";
import type { Item } from "@/lib/chat";
import { useConversation } from "@/hooks/queries";
import { errorText } from "@/lib/backend";
import { Button } from "@/components/ui/button";
import { SkeletonLines, Skeleton } from "@/components/ui/skeleton";
import { ErrorNote } from "@/components/common";
import { TaskTimeline } from "@/components/task/TaskTimeline";
import { ToolGroup } from "@/components/task/ToolGroup";
import { Composer } from "./Composer";
import { ContextControls } from "./ContextControls";
import { AssistantMessage, ErrorBanner, Notice, Thinking, UserMessage } from "./Messages";
import { cn } from "@/lib/utils";

const EMPTY: Item[] = [];
const VirtualTranscript = lazy(() => import("./VirtualTranscript"));
/** Below this many items the transcript renders directly (content-visibility skips off-screen turns);
 *  above it, TanStack Virtual keeps only the visible turns in the DOM. */
export const VIRTUALIZE_AT = 40;

export function ChatView({ convId }: { convId: string }) {
  const chat = useApp((s) => s.chats[convId]);
  const ensureChat = useApp((s) => s.ensureChat);
  const conv = useConversation(chat?.loaded || chat?.items.length ? null : convId);
  useEffect(() => {
    if (conv.data) ensureChat(convId, conv.data.items);
  }, [conv.data, convId, ensureChat]);

  const items = chat?.items ?? EMPTY;
  const working = useApp((s) => s.activeRun?.conv_id === convId || s.sending === convId);
  const liveRun = useApp((s) => (s.activeRun?.conv_id === convId ? s.activeRun.run_id : null));
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const [showJump, setShowJump] = useState(false);
  const virtual = items.length > VIRTUALIZE_AT;

  const scrollToEnd = useRef<(() => void) | null>(null);

  const toBottom = useCallback((smooth = false) => {
    const el = scroller.current;
    if (!el) return;
    scrollToEnd.current?.();
    el.scrollTo({ top: el.scrollHeight, behavior: smooth ? "smooth" : "auto" });
  }, []);

  // Follow new content while the user is at the bottom; stop following when they scroll up.
  useLayoutEffect(() => {
    if (stick.current) toBottom();
  }, [items, working, toBottom]);

  useEffect(() => {
    const el = content.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      if (stick.current) toBottom();
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [toBottom]);

  // When the user sends a message, always jump back down.
  const lastUserId = [...items].reverse().find((i) => i.kind === "user")?.id;
  useEffect(() => {
    stick.current = true;
    setShowJump(false);
    toBottom();
  }, [lastUserId, toBottom]);

  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
    stick.current = dist < 48;
    setShowJump(dist > 240);
  };

  const last = items.at(-1);
  let lastAssistantIdx = -1;
  for (let i = items.length - 1; i >= 0; i--)
    if (items[i]!.kind === "assistant") {
      lastAssistantIdx = i;
      break;
    }
  const showThinking =
    working &&
    (!last ||
      last.kind === "user" ||
      last.kind === "notice" ||
      (last.kind === "assistant" && !last.streaming) ||
      (last.kind === "tools" && last.finished_at !== null));

  const renderItem = (it: Item, idx: number): ReactNode => {
    switch (it.kind) {
      case "user":
        return <UserMessage item={it} />;
      case "assistant":
        return <AssistantMessage item={it} convId={convId} isLast={idx === lastAssistantIdx && idx >= items.length - 1} canRetry={!working} />;
      case "tools":
        return <ToolGroup item={it} />;
      case "task":
        return (
          <TaskTimeline
            item={it}
            live={liveRun === it.run_id}
            onStop={() => useApp.getState().stop()}
            onRetry={!working ? () => void useApp.getState().retry(convId) : undefined}
          />
        );
      case "notice":
        return <Notice item={it} />;
      case "error":
        return <ErrorBanner item={it} convId={convId} isLast={idx === items.length - 1 && !working} />;
    }
  };

  const loading = !chat && !conv.isError;

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div ref={scroller} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto scroll-thin" role="log" aria-label="Conversation" aria-live="off">
        <div ref={content} className="mx-auto w-full max-w-(--relay-content-max) px-4 pt-2 pb-10 sm:px-6">
          {loading && <LoadingTurns />}
          {conv.isError && <ErrorNote title="Couldn't open this chat">{errorText(conv.error)}</ErrorNote>}
          {chat?.error && <ErrorNote title="Couldn't start this chat">{chat.error}</ErrorNote>}
          {virtual ? (
            <Suspense fallback={items.slice(-VIRTUALIZE_AT).map((it, i) => <div key={it.id}>{renderItem(it, items.length - VIRTUALIZE_AT + i)}</div>)}>
              <VirtualTranscript items={items} scroller={scroller} renderItem={renderItem} scrollToEndRef={scrollToEnd} />
            </Suspense>
          ) : (
            items.map((it, idx) => <div key={it.id}>{renderItem(it, idx)}</div>)
          )}
          {showThinking && <Thinking />}
        </div>
      </div>

      <div
        className={cn(
          "pointer-events-none absolute bottom-[132px] left-1/2 z-(--relay-z-sticky) -translate-x-1/2 transition-[opacity,transform] duration-(--relay-dur)",
          showJump ? "translate-y-0 opacity-100" : "translate-y-2 opacity-0",
        )}
      >
        <Button
          aria-label="Scroll to the latest message"
          tabIndex={showJump ? 0 : -1}
          aria-hidden={!showJump}
          size="icon-lg"
          onClick={() => {
            stick.current = true;
            toBottom(true);
          }}
          className="pointer-events-auto rounded-full shadow-pop"
        >
          <ArrowDown aria-hidden="true" />
        </Button>
      </div>

      <div className="shrink-0 px-3 pb-3 sm:px-4">
        <div className="mx-auto w-full max-w-(--relay-content-max)">
          <Composer convId={convId} autoFocus placeholder="Reply to Relay" />
          <ContextControls size="sm" className="mt-2" />
        </div>
      </div>
    </div>
  );
}

function LoadingTurns() {
  return (
    <div className="space-y-6 py-8" aria-label="Loading chat" role="status">
      <Skeleton className="ml-auto h-10 w-1/2 rounded-xl" />
      <SkeletonLines n={3} />
      <Skeleton className="h-28 w-full rounded-xl" />
    </div>
  );
}
