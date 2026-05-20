ALTER TABLE project_members
ADD COLUMN agent_type TEXT NOT NULL DEFAULT 'reasoning'
CHECK (agent_type IN ('reasoning', 'writer'));
