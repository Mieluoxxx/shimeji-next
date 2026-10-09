//! 与平台无关的模拟层：所有坐标为逻辑像素，时间为秒。

pub mod behaviors;
pub mod clips;
pub mod engine;
pub mod env;
pub mod mascot;
pub mod world;

pub use world::World;
