'use client';

import { Avatar } from '@heroui/react';
import { createContext, useContext } from 'react';
import { DEFAULT_BRANDING, type Branding } from '../lib/branding';

const BrandingContext = createContext<Branding>(DEFAULT_BRANDING);

/** Hands the instance branding (read on the server per request) to client components. */
export function BrandingProvider({
  branding,
  children,
}: {
  branding: Branding;
  children: React.ReactNode;
}) {
  return (
    <BrandingContext.Provider value={branding}>
      {children}
    </BrandingContext.Provider>
  );
}

export const useBranding = () => useContext(BrandingContext);

/** The instance's logo (its favicon, or the first letter of its title) and name. */
export function Brand({
  size = 'sm',
  hideNameOnMobile,
  nameClassName = 'text-base font-semibold',
}: {
  size?: 'sm' | 'md';
  hideNameOnMobile?: boolean;
  nameClassName?: string;
}) {
  const b = useBranding();
  return (
    <>
      <Avatar
        size={size}
        className="rounded-sm bg-accent text-accent-foreground"
        data-testid="brand-logo"
      >
        {b.has_favicon && (
          <Avatar.Image
            alt=""
            src={`/api/v1/branding/favicon?v=${b.version}`}
          />
        )}
        <Avatar.Fallback className="rounded-sm bg-accent font-bold text-accent-foreground">
          {b.title.slice(0, 1).toLowerCase()}
        </Avatar.Fallback>
      </Avatar>
      <span
        className={`${nameClassName} ${hideNameOnMobile ? 'hidden sm:inline' : ''}`}
        data-testid="brand-name"
      >
        {b.title}
      </span>
    </>
  );
}
