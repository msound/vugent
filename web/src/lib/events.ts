// The live updates of ADR 0008, from the server's `/api/events` stream.

import type { Change } from './live';

export interface Listeners {
  /** The stream opened or reopened; events may have been missed meanwhile. */
  onopen: () => void;
  onchange: (change: Change) => void;
  /** Whether the server polls for changes rather than watching files. */
  onmode: (polling: boolean) => void;
}

/** Listens to the server's changes until the returned function is called. The browser reconnects by itself. */
export function listen(listeners: Listeners): () => void {
  const source = new EventSource('/api/events');
  source.addEventListener('open', listeners.onopen);
  source.addEventListener('files', (event) => {
    const { paths } = JSON.parse(event.data) as { paths: string[] | null };
    listeners.onchange({ kind: 'files', paths });
  });
  source.addEventListener('index', () => listeners.onchange({ kind: 'index' }));
  source.addEventListener('refs', () => listeners.onchange({ kind: 'refs' }));
  source.addEventListener('mode', (event) => {
    const { polling } = JSON.parse(event.data) as { polling: boolean };
    listeners.onmode(polling);
  });
  return () => source.close();
}
