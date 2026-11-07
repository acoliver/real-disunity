//! Names the caller gives the resources of an offscreen frame.

macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u64);
    };
}

id!(
    /// A shader the caller registered with the offscreen state.
    ShaderId
);
id!(
    /// A material of a frame.
    MaterialId
);
id!(
    /// A texture, texture array or render texture. One name space holds all three, since a
    /// blit or a property can name any of them.
    TextureId
);
id!(
    /// A mesh of a frame.
    MeshId
);
id!(
    /// A camera of a frame.
    CameraId
);
id!(
    /// A renderer of a frame.
    RendererId
);
