//! SVG presentation of ELK coordinates, without a second layout engine.

mod crossings;
mod drawing;
mod render;
mod route;
mod wave;

pub use render::render;

#[cfg(test)]
mod tests;
