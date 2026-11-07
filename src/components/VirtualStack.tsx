import { useVirtualizer } from '@tanstack/react-virtual';
import { type ReactNode, useLayoutEffect, useRef, useState } from 'react';
import { useScrollRoot } from '../shell/scroll';

/**
 * A virtualized column of items with different heights (measured after
 * render), scrolling with the content panel like `TrackList`.
 */
export function VirtualStack<T>({
  items,
  itemKey,
  estimate,
  render,
}: {
  items: T[];
  itemKey: (item: T) => string;
  estimate: number;
  render: (item: T) => ReactNode;
}) {
  const root = useScrollRoot();
  const list = useRef<HTMLDivElement>(null);
  const [margin, setMargin] = useState(0);

  // The list's offset inside the scrolled content; content above it may grow.
  useLayoutEffect(() => {
    const scroller = root.current;
    const element = list.current;
    if (!scroller || !element) return;
    const measure = () => {
      const top =
        element.getBoundingClientRect().top -
        scroller.getBoundingClientRect().top +
        scroller.scrollTop;
      setMargin((previous) => (Math.abs(previous - top) < 1 ? previous : top));
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (scroller.firstElementChild) observer.observe(scroller.firstElementChild);
    return () => observer.disconnect();
  }, [root]);

  const rows = useVirtualizer({
    count: items.length,
    getScrollElement: () => root.current,
    estimateSize: () => estimate,
    getItemKey: (index) => itemKey(items[index]),
    overscan: 4,
    scrollMargin: margin,
  });

  return (
    <div ref={list} className="relative" style={{ height: rows.getTotalSize() }}>
      {rows.getVirtualItems().map((row) => (
        <div
          key={row.key}
          data-index={row.index}
          ref={rows.measureElement}
          className="absolute left-0 w-full has-[[data-menu]]:z-10"
          style={{ transform: `translateY(${row.start - margin}px)` }}
        >
          {render(items[row.index])}
        </div>
      ))}
    </div>
  );
}
