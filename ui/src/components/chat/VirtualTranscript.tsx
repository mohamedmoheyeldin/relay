// Long transcripts (more than VIRTUALIZE_AT items): only the visible turns are in the DOM.
// Loaded on demand so short chats (the common case) do not pay for the virtualizer.
import { useEffect, type ReactNode, type RefObject } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Item } from "@/lib/chat";

export default function VirtualTranscript({
  items,
  scroller,
  renderItem,
  scrollToEndRef,
}: {
  items: Item[];
  scroller: RefObject<HTMLDivElement | null>;
  renderItem: (it: Item, idx: number) => ReactNode;
  /** Receives a function that scrolls to the last item (used by stick-to-bottom). */
  scrollToEndRef: RefObject<(() => void) | null>;
}) {
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 140,
    overscan: 6,
    getItemKey: (i) => items[i]?.id ?? i,
  });
  useEffect(() => {
    scrollToEndRef.current = () => {
      if (items.length) virtualizer.scrollToIndex(items.length - 1, { align: "end" });
    };
    return () => {
      scrollToEndRef.current = null;
    };
  }, [items.length, virtualizer, scrollToEndRef]);
  return (
    <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
      {virtualizer.getVirtualItems().map((v) => (
        <div key={v.key} data-index={v.index} ref={virtualizer.measureElement} className="absolute top-0 left-0 w-full" style={{ transform: `translateY(${v.start}px)` }}>
          {renderItem(items[v.index]!, v.index)}
        </div>
      ))}
    </div>
  );
}
