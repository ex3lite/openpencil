//! Document wire serialization shares the canonical deduplicated image table.
//!
//! Plain typed serialization repeats each inline ImageSrc payload for every
//! reference. A 55 MB design system can consequently exceed the transport cap.
//! Flattening streams typed fields first and then the collected unique images,
//! without a document-sized JSON Value or expanded intermediate string.

use op_editor_core::PenDocument;
use serde::Serialize;

pub fn write_transport_document<W: std::io::Write>(
    writer: &mut W,
    document: &PenDocument,
) -> serde_json::Result<()> {
    #[derive(Serialize)]
    struct TransportDocument<'a, 'b> {
        #[serde(flatten)]
        document: &'a PenDocument,
        images: jian_ops_schema::image_table::SaveImageTable<'b>,
    }
    let thumbnails = jian_ops_schema::image_thumbs::capture_snapshot();
    jian_ops_schema::image_table::with_save_scope(&thumbnails, |tables| {
        serde_json::to_writer(
            writer,
            &TransportDocument {
                document,
                images: tables.images(),
            },
        )
    })
}
