#[cfg(test)]
mod tests;

// The attachment metadata can come from 3 different places.
//
// 1. Inline attachment metadata on conversations/messages. This not complete but is enough for
// clients to display basic information about the attachments
// 2. Attachment Metadata request. This is 98% complete and contains everything except for some
// missing headers.
// 3. Get Message request. This includes 80% of the attachment data and the attachment headers.
// currently this is the only place where we will find these headers.
//
// The attachment data is all stored in one table and initialized partially with data from all
// these sources.
