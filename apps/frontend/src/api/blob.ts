import { currentAccessToken, refreshAccessToken } from './fetcher';

export async function apiFetchBlob(url: string): Promise<Blob> {
  const go = () => fetch(url, { headers: currentAccessToken() ? { Authorization: `Bearer ${currentAccessToken()}` } : {}, credentials: 'same-origin' });
  let res = await go();
  if (res.status === 401 && (await refreshAccessToken())) res = await go();
  if (!res.ok) throw new Error(`download failed: ${res.status}`);
  return res.blob();
}
