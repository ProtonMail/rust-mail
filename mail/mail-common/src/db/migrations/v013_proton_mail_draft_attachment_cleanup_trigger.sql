-- Delete attachments that were not uploaded to the server on draft discard.
CREATE TRIGGER cleanup_draft_attachments_trigger AFTER DELETE ON draft_attachment_metadata
    BEGIN
        DELETE FROM attachments WHERE local_id = OLD.local_attachment_id AND OLD.state <> 1;
    END;
