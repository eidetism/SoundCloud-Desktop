import { useEffect, useState } from 'react';
import { getSession, onSessionChange, type Session } from './api';

/** The Rust-owned session; `null` until the first answer. */
export function useSession(): Session | null {
  const [session, setSession] = useState<Session | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = onSessionChange((next) => {
      if (active) setSession(next);
    });
    getSession().then((current) => {
      if (active) setSession((previous) => previous ?? current);
    });
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  return session;
}
