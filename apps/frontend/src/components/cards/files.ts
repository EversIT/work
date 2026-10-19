import { apiFetchBlob } from '../../api/blob';

/** Files are authenticated: fetch with the bearer token, then hand the bytes to the browser. */
export async function downloadFile(id: string, name: string) {
  const blob = await apiFetchBlob(`/api/v1/files/${id}`);
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.rel = 'noopener';
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
