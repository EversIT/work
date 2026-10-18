import Link from 'next/link';
import { getTranslations } from 'next-intl/server';
import { Card, Button } from '@heroui/react';
export default async function NotFound() {
  const t = await getTranslations('notFound');
  return (
    <>
      <main className="flex min-h-[calc(100vh-3.5rem)] items-center justify-center px-4 py-12">
        <div className="w-full max-w-md">
          <Card className="text-center shadow-xl">
            <Card.Header className="flex-col items-center gap-3 pb-2">
              <p className="text-6xl font-semibold tracking-tight text-accent">
                404
              </p>
              <Card.Title className="text-xl">{t('title')}</Card.Title>
              <Card.Description className="text-base">
                {t('description')}
              </Card.Description>
            </Card.Header>

            <Card.Footer className="justify-center">
              <Button variant="outline">
                <Link href="/">{t('backHome')}</Link>
              </Button>
            </Card.Footer>
          </Card>
        </div>
      </main>
    </>
  );
}
