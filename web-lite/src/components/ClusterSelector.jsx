import { cluster, customUrl } from '../state.js';
import { CLUSTER_OPTIONS } from '../rpc.js';

export function ClusterSelector() {
  const handleClusterChange = (e) => {
    cluster.value = e.target.value;
  };

  const handleUrlChange = (e) => {
    customUrl.value = e.target.value;
  };

  return (
    <div class="cluster-selector">
      <select value={cluster.value} onChange={handleClusterChange}>
        {CLUSTER_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      {cluster.value === 'custom' && (
        <input
          type="text"
          placeholder="https://..."
          value={customUrl.value}
          onInput={handleUrlChange}
        />
      )}
    </div>
  );
}
