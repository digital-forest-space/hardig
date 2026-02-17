import { useRef, useEffect } from 'preact/hooks';
import { logs } from '../state.js';

export function LogPanel() {
  const ref = useRef(null);
  const entries = logs.value;

  useEffect(() => {
    if (ref.current) {
      ref.current.scrollTop = ref.current.scrollHeight;
    }
  }, [entries.length]);

  return (
    <div class="log-panel" ref={ref}>
      {entries.map((entry, i) => (
        <div key={i} class={`log-entry ${entry.isError ? 'error' : ''}`}>
          {entry.text}
        </div>
      ))}
    </div>
  );
}
