// Language providers and typed boundary adapters share one verified source
// coordinate authority. Keep this narrow alias so the language adapter does
// not own a second implementation.
pub(super) use crate::static_pipeline::source_evidence::VerifiedSourceFile as SourceCoordinates;
