//! The wind map's data: model fields from GRIB2 files (ICON-D2 from the DWD; ECMWF to come),
//! turned into PNG textures for the particle layer, one per model run and forecast hour.
//! The texture format follows mapbox/webgl-wind (`data/prepare.js`): red is U, green is V,
//! each scaled to the file's own range, which a JSON file next to it gives.

pub mod grid;
pub mod icon_d2;
pub mod index;
pub mod service;
pub mod store;
#[doc(hidden)]
pub mod testing;
pub mod texture;

pub use index::{MapFrame, MapIndex, MapLayer};
pub use service::{MapData, Urls};

/// The models on the map, finest first.
pub const MODELS: &[&str] = &[icon_d2::MODEL];
