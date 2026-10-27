use super::{MaterialHandle, SceneValidationError};

/// One material binding over a nonempty triangle-aligned index interval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneLodMaterialSlot {
    material: MaterialHandle,
    indices: std::ops::Range<u32>,
}

impl SceneLodMaterialSlot {
    pub fn new(
        material: MaterialHandle,
        indices: std::ops::Range<u32>,
    ) -> Result<Self, SceneValidationError> {
        if indices.start >= indices.end
            || !indices.start.is_multiple_of(3)
            || !indices.end.is_multiple_of(3)
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "LOD material slot requires whole nonempty triangles",
            });
        }
        Ok(Self { material, indices })
    }

    pub const fn material(&self) -> MaterialHandle {
        self.material
    }

    pub fn indices(&self) -> std::ops::Range<u32> {
        self.indices.clone()
    }
}

pub(super) fn validate_slots(
    slots: &[SceneLodMaterialSlot],
    index_count: usize,
) -> Result<(), SceneValidationError> {
    let mut end = 0;
    for slot in slots {
        if slot.indices.start != end {
            return Err(SceneValidationError::UnsupportedInput {
                part: "LOD material slots must partition the index buffer",
            });
        }
        end = slot.indices.end;
    }
    if slots.is_empty() || u64::from(end) != index_count as u64 {
        return Err(SceneValidationError::UnsupportedInput {
            part: "LOD material slots must cover the index buffer",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_nonempty_triangle_partitions() {
        let material = MaterialHandle::from_raw(2);
        for range in [0..0, std::ops::Range { start: 3, end: 0 }, 1..3, 0..4] {
            assert!(SceneLodMaterialSlot::new(material, range).is_err());
        }
        let a = SceneLodMaterialSlot::new(material, 0..3).unwrap();
        let b = SceneLodMaterialSlot::new(material, 3..9).unwrap();
        assert_eq!(b.material().raw(), 2);
        assert_eq!(b.indices(), 3..9);
        assert!(validate_slots(&[a.clone(), b.clone()], 9).is_ok());
        assert!(validate_slots(&[], 9).is_err());
        assert!(validate_slots(std::slice::from_ref(&b), 9).is_err());
        assert!(validate_slots(&[a.clone(), a.clone()], 6).is_err());
        assert!(validate_slots(&[a.clone(), b], 6).is_err());
        assert!(validate_slots(&[a], 6).is_err());
    }
}
