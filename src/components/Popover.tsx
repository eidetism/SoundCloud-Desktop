import { type ReactNode, useEffect, useRef } from 'react';

/** A dropdown that closes on an outside click or Escape. */
export function Popover({
  onClose,
  className,
  children,
}: {
  onClose: () => void;
  className: string;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const close = (event: PointerEvent) => {
      if (!box.current?.parentElement?.contains(event.target as Node)) onClose();
    };
    const key = (event: KeyboardEvent) => event.key === 'Escape' && onClose();
    document.addEventListener('pointerdown', close);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('pointerdown', close);
      document.removeEventListener('keydown', key);
    };
  }, [onClose]);

  return (
    <div
      ref={box}
      data-menu
      className={`absolute z-20 overflow-hidden rounded-md border border-line bg-bg py-1 shadow-[0_8px_24px_var(--shade)] ${className}`}
    >
      {children}
    </div>
  );
}

/** A plain menu row. */
export function MenuItem({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center gap-2 px-3.5 py-2 text-left text-xs hover:bg-hover"
    >
      {children}
    </button>
  );
}
