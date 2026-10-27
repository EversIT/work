-- Where a session was last used from, so the user can recognise it in the session list.
ALTER TABLE sessions
    ADD COLUMN user_agent text CHECK (char_length(user_agent) <= 300),
    ADD COLUMN ip         text CHECK (char_length(ip) <= 64);
