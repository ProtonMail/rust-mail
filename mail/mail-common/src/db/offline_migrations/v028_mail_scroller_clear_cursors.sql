-- Clear conversation scroller cursors to force refresh after create_or_get_local fix
-- This ensures conversations that were lost due to the unknown conversation bug
-- will be re-fetched from the API with proper label data
DELETE FROM mail_conversation_scroll_data;
