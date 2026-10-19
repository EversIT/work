import { Avatar } from '@heroui/react';
import { initials } from '../../lib/format';

export function UserAvatar({
  user,
  size = 'sm',
}: {
  user: { first_name: string; last_name: string };
  size?: 'sm' | 'md' | 'lg';
}) {
  return (
    <Avatar size={size}>
      <Avatar.Fallback>{initials(user)}</Avatar.Fallback>
    </Avatar>
  );
}
