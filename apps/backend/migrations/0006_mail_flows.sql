-- Invites by e-mail are bound to the invited address: only that account can redeem them.
ALTER TABLE invites ADD COLUMN email citext;
