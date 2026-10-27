-- Date-based automation triggers and the scheduler that fires them.

ALTER TABLE automations DROP CONSTRAINT automations_trigger_type_check;
ALTER TABLE automations ADD CONSTRAINT automations_trigger_type_check
    CHECK (trigger_type IN ('card_created','field_updated','stage_changed','comment_created','assignee_added','assignee_removed','webhook','date_arrives'));

-- One row per (automation, card, date value) that already fired, so a date fires once, and fires
-- again when somebody moves it.
CREATE TABLE automation_date_fires (
    automation_id uuid NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    card_id       uuid NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    fire_at       timestamptz NOT NULL,
    fired_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (automation_id, card_id, fire_at)
);

-- Where the scheduler stopped last time (one row per job).
CREATE TABLE scheduler_state (
    job      text PRIMARY KEY,
    last_run timestamptz NOT NULL
);
