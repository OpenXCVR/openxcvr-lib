pub mod catalog;
mod enums;
pub mod media_interface;
mod pages;
pub mod revision;
pub mod state_machines;

pub use media_interface::emit_media_interface;
pub use state_machines::{
    emit_shared_types_header, emit_state_machine_c_header, emit_state_machine_c_source,
    SHARED_HEADER_PATH,
};

use crate::emitters::Emitter;
use crate::ir::enums::EnumSchema;
use crate::ir::pages::PageSchema;

pub struct CEmitter;

impl CEmitter {
    pub fn emit_page_header(&self, schema: &PageSchema) -> String {
        pages::emit_page_header(schema)
    }

    pub fn emit_page_source(&self, schema: &PageSchema, header_path: &str) -> String {
        pages::emit_page_source(schema, header_path)
    }
}

impl Emitter for CEmitter {
    fn emit_enum(&self, schema: &EnumSchema) -> String {
        enums::emit_enum(schema)
    }

    fn emit_page(&self, schema: &PageSchema) -> String {
        self.emit_page_header(schema)
    }
}
