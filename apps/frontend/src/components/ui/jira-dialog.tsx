'use client';

import { Modal, useOverlayState } from '@heroui/react';

const WIDTH = { sm: 'max-w-sm', md: 'max-w-xl', lg: 'max-w-3xl', xl: 'max-w-5xl' } as const;

/**
 * The one modal shell of the app, shaped like Jira's dialogs: a fixed-width white panel with a
 * title bar, a scrolling body and a footer with right-aligned actions.
 */
export function JiraDialog({
  isOpen,
  onClose,
  title,
  children,
  footer,
  size = 'md',
  header,
  testId,
}: {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  /** Replaces the default title bar (used by the issue view). */
  header?: React.ReactNode;
  children: React.ReactNode;
  footer?: React.ReactNode;
  size?: keyof typeof WIDTH;
  testId?: string;
}) {
  const state = useOverlayState({
    isOpen,
    onOpenChange: (open) => {
      if (!open) onClose();
    },
  });
  return (
    <Modal state={state}>
      <Modal.Backdrop className="bg-[#091E42]/60">
        <Modal.Container size="full" placement="top" scroll="inside" className="items-start justify-center p-4 pt-[6vh]">
          <Modal.Dialog
            aria-label={title}
            data-testid={testId}
            className={`mx-auto flex max-h-[88vh] w-full flex-col overflow-hidden rounded-[6px] bg-white p-0 shadow-[0_8px_12px_#091e4226,0_0_1px_#091e424f] ${WIDTH[size]}`}
          >
            {header ?? (
              <div className="flex items-center justify-between px-6 pb-2 pt-5">
                <h2 className="text-xl font-medium text-[#172B4D]">{title}</h2>
                <Modal.CloseTrigger className="static" />
              </div>
            )}
            <div className="min-h-0 flex-1 overflow-y-auto px-6 py-3">{children}</div>
            {footer && <div className="flex items-center justify-end gap-2 px-6 pb-5 pt-3">{footer}</div>}
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

export { useOverlayState };
