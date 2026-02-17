import { Connection } from '@solana/web3.js';

const CLUSTER_URLS = {
  localnet: 'http://localhost:8899',
  devnet: 'https://api.devnet.solana.com',
  'mainnet-beta': 'https://api.mainnet-beta.solana.com',
};

export function getClusterUrl(cluster) {
  return CLUSTER_URLS[cluster] || cluster;
}

export function createConnection(cluster) {
  const url = getClusterUrl(cluster);
  return new Connection(url, 'confirmed');
}

export const CLUSTER_OPTIONS = [
  { value: 'localnet', label: 'Localnet' },
  { value: 'devnet', label: 'Devnet' },
  { value: 'mainnet-beta', label: 'Mainnet' },
  { value: 'custom', label: 'Custom' },
];
