import { Card, buttonVariants } from '@heroui/react';
import { getTranslations } from 'next-intl/server';
import Link from 'next/link';
import { Brand } from '../components/brand';

/**
 * Shown for any address that does not exist. Laid out like the sign-in pages (grey page, white card with the
 * same shadow, the instance's brand on top), so a wrong link does not look like another website.
 */
export default async function NotFound() {
  const t = await getTranslations('notFound');
  return (
    <main
      className="flex min-h-screen items-start justify-center bg-surface-secondary px-4 pt-[12vh]"
      data-testid="not-found"
    >
      <Card className="w-full max-w-sm shadow-[0_8px_12px_#091e4226,0_0_1px_#091e424f]">
        <Card.Header>
          <div className="mb-2 flex items-center gap-2">
            <Brand size="sm" nameClassName="text-base font-semibold" />
          </div>
          <p
            aria-hidden
            className="text-5xl font-semibold tracking-tight text-accent"
          >
            404
          </p>
          <Card.Title>{t('title')}</Card.Title>
          <Card.Description>{t('description')}</Card.Description>
        </Card.Header>
        <Card.Footer>
          <Link href="/" className={buttonVariants({ variant: 'primary' })}>
            {t('backHome')}
          </Link>
        </Card.Footer>
      </Card>
    </main>
  );
}
