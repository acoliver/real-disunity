//! Decoding one synthetic object, for the object layout tests.

use worldrunner_unity_serialized::{
    ClassId, SerializedError, SerializedFile,
    object::{Layout, decode, object_bytes},
};

use crate::support::{class, object, scene};

/// Writes one object of `class_id` into a file, then decodes it with `T`'s layout.
pub fn decode_one<T: Layout>(
    class_id: ClassId,
    data: Vec<u8>,
    big_endian: bool,
) -> Result<T, SerializedError> {
    let mut contents = scene(22);
    contents.big_endian = big_endian;
    // The scene already declares a MonoBehaviour type with its script ID, at index 2.
    let type_index = if class_id == ClassId::MONO_BEHAVIOUR {
        2
    } else {
        contents.types.push(class(class_id));
        i32::try_from(contents.types.len() - 1).unwrap()
    };
    contents.objects = vec![object(7, type_index, data)];
    let bytes = contents.write().unwrap();
    let file = SerializedFile::parse(&bytes).unwrap();
    let info = file.object(7).unwrap();
    decode(object_bytes(&bytes, info).unwrap(), info, big_endian)
}
