-- The self-hosted language model an admin entered in the app (Administration -> AI). One row at
-- most. It wins over the WORK_AI_MODEL_* variables; without a row and without variables the AI
-- features stay off. There is no API key: the model runs on the operator's own infrastructure.
CREATE TABLE ai_settings (
    singleton         boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    model_url         text NOT NULL CHECK (char_length(model_url) BETWEEN 8 AND 500),
    model_name        text NOT NULL CHECK (char_length(model_name) BETWEEN 1 AND 100),
    max_output_tokens integer NOT NULL DEFAULT 1024 CHECK (max_output_tokens BETWEEN 64 AND 8000),
    updated_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_at        timestamptz NOT NULL DEFAULT now()
);
