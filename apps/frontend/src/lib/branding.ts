export type Branding = {
  title: string;
  description: string;
  accent: string;
  has_favicon: boolean;
  version: number;
};

export const DEFAULT_BRANDING: Branding = {
  title: 'work',
  description: 'Open-source work management',
  accent: '#0C66E4',
  has_favicon: false,
  version: 0,
};

/**
 * The instance's name, description, accent colour and favicon, read on the server for every page
 * (the backend answer is public). If the backend cannot be reached the defaults keep pages working.
 */
export async function getBranding(): Promise<Branding> {
  const backend = process.env.BACKEND_URL || 'http://localhost:3001';
  try {
    const res = await fetch(`${backend}/api/v1/branding`, {
      cache: 'no-store',
      signal: AbortSignal.timeout(2000),
    });
    if (!res.ok) return DEFAULT_BRANDING;
    const b = (await res.json()) as Partial<Branding>;
    return {
      title:
        typeof b.title === 'string' && b.title
          ? b.title
          : DEFAULT_BRANDING.title,
      description:
        typeof b.description === 'string'
          ? b.description
          : DEFAULT_BRANDING.description,
      accent: typeof b.accent === 'string' ? b.accent : DEFAULT_BRANDING.accent,
      has_favicon: b.has_favicon === true,
      version: typeof b.version === 'number' ? b.version : 0,
    };
  } catch {
    return DEFAULT_BRANDING;
  }
}
