CREATE UNIQUE INDEX IF NOT EXISTS idx_files_project_filename ON files(project_id, filename);
