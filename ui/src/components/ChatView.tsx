import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowDown } from "lucide-react";
import { useApp } from "../store/app";
import type { Item } from "../lib/chat";
import { Composer } from "./Composer";
import { AssistantMessage, ErrorBanner, Notice, Thinking, ToolGroup, UserMessage } from "./Messages";
import { Button, cn } from "./ui";

const EMPTY: Item[] = [];

export function ChatView({ convId }: { convId: string }) {
  const items = useApp((s) => s.chats[convId]?.items ?? EMPTY);
  const loaded = useApp((s) => s.chats[convId]?.loaded ?? false);
  const error = useApp((s) => s.chats[convId]?.error ?? null);
  const working = useApp((s) => s.activeRun?.conv_id === convId || s.sending === convId);
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const [showJump, setShowJump] = useState(false);

  const toBottom = useCallback((smooth = false) => {
    const el = scroller.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: smooth ? "smooth" : "auto" });
  }, []);

  // Follow new content while the user is at the bottom; stop following when they scroll up.
  useLayoutEffect(() => {
    if (stick.current) toBottom();
  }, [items, working, toBottom]);

  useEffect(() => {
    const el = content.current;
    if (!el) return;
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
    stick.current = dist < 40;
    setShowJump(dist > 200);
  };

  const last = items.at(-1);
  const lastAssistantIdx = (() => {
    for (let i = items.length - 1; i >= 0; i--) if (items[i]!.kind === "assistant") return i;
    return -1;
  })();
  const showThinking =
    working &&
    (!last ||
      last.kind === "user" ||
      last.kind === "notice" ||
      (last.kind === "assistant" && !last.streaming) ||
      (last.kind === "tools" && last.finished_at !== null));

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div ref={scroller} onScroll={onScroll} className="scroll-thin min-h-0 flex-1 overflow-y-auto" role="log" aria-label="Conversation">
        <div ref={content} className="mx-auto w-full max-w-[768px] px-4 pt-2 pb-8 sm:px-6">
          {!loaded && items.length === 0 && <LoadingTurns />}
          {error && (
            <div role="alert" className="my-8 rounded-2xl bg-danger-soft px-4 py-3 text-[14px]">
              Couldn't open this chat: {error}
            </div>
          )}
          {items.map((it, idx) => {
            switch (it.kind) {
              case "user":
                return <UserMessage key={it.id} item={it} />;
              case "assistant":
                return (
                  <AssistantMessage
                    key={it.id}
                    item={it}
                    convId={convId}
                    isLast={idx === lastAssistantIdx && idx >= items.length - 1}
                    canRetry={!working}
                  />
                );
              case "tools":
                return <ToolGroup key={it.id} item={it} />;
              case "notice":
                return <Notice key={it.id} item={it} />;
              case "error":
                return <ErrorBanner key={it.id} item={it} convId={convId} isLast={idx === items.length - 1 && !working} />;
            }
          })}
          {showThinking && <Thinking />}
        </div>
      </div>

      <div
        className={cn(
          "pointer-events-none absolute bottom-[112px] left-1/2 z-10 -translate-x-1/2 transition-[opacity,transform] duration-150",
          showJump ? "translate-y-0 opacity-100" : "translate-y-2 opacity-0",
        )}
      >
        <Button
          aria-label="Scroll to bottom"
          tabIndex={showJump ? 0 : -1}
          aria-hidden={!showJump}
          onClick={() => {
            stick.current = true;
            toBottom(true);
          }}
          className="pointer-events-auto h-9 w-9 !px-0 bg-bg shadow-[var(--shadow)]"
        >
          <ArrowDown size={17} />
        </Button>
      </div>

      <div className="shrink-0 px-3 pb-2 sm:px-4">
        <div className="mx-auto w-full max-w-[768px]">
          <Composer autoFocus placeholder="Ask anything" />
          <p className="py-2 text-center text-[12px] text-subtle">Relay can make mistakes. Check important info.</p>
        </div>
      </div>
    </div>
  );
}

function LoadingTurns() {
  return (
    <div className="space-y-6 py-6" aria-label="Loading chat">
      <div className="ml-auto skeleton h-10 w-1/2 rounded-[22px]" />
      <div className="space-y-2.5">
        <div className="skeleton h-4 w-11/12" />
        <div className="skeleton h-4 w-4/5" />
        <div className="skeleton h-4 w-2/3" />
      </div>
    </div>
  );
}
