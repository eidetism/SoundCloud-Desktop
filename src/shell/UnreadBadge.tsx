import { useUnread } from '../api/messages';

/** Count of unread conversations; nothing when there are none. */
export function UnreadBadge({ className = '' }: { className?: string }) {
  const count = useUnread((state) => state.count);
  if (count === 0) return null;
  return (
    <span
      className={`grid h-[18px] min-w-[18px] place-items-center rounded-full bg-danger px-1 text-[10px] leading-none font-bold text-on-accent ${className}`}
    >
      {count > 9 ? '9+' : count}
    </span>
  );
}
