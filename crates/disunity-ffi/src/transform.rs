use crate::{DisunityObject, DisunityWorld};
use glam::{EulerRot, Quat, Vec3};
use serde_json::{Value, json};

impl DisunityObject {
    pub(crate) fn rotation(&self) -> Quat {
        Quat::from_euler(
            EulerRot::XYZ,
            self.rotation_x_radians,
            self.rotation_y_radians,
            self.rotation_z_radians,
        )
    }
    pub(crate) fn set_rotation(&mut self, q: Quat) {
        let (x, y, z) = q.normalize().to_euler(EulerRot::XYZ);
        self.rotation_x_radians = x;
        self.rotation_y_radians = y;
        self.rotation_z_radians = z;
    }
}
impl DisunityWorld {
    pub(crate) fn object_index(&self, handle: u64) -> Result<usize, String> {
        let (index, generation) = crate::split_handle(handle)?;
        if self.object_generations.get(index) != Some(&generation)
            || self.objects.get(index).and_then(Option::as_ref).is_none()
        {
            return Err("object handle is stale or belongs to another world".into());
        }
        Ok(index)
    }
    pub(crate) fn world_trs(&self, handle: u64) -> Result<(Vec3, Quat, Vec3), String> {
        let mut chain = Vec::new();
        let mut current = Some(handle);
        while let Some(h) = current {
            let i = self.object_index(h)?;
            chain.push((h, i));
            current = self.metadata[&h].parent;
            if chain.len() > 256 {
                return Err("hierarchy depth exceeds 256".into());
            }
        }
        let (mut p, mut r, mut s) = (Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
        for (h, i) in chain.into_iter().rev() {
            let o = self.objects[i].as_ref().unwrap();
            let (lp, lr, ls) = self.local_trs(h, o)?;
            p += r * (s * lp);
            r = (r * lr).normalize();
            s *= ls;
        }
        Ok((p, r, s))
    }
    pub(crate) fn transform_snapshot(&self, handle: u64) -> Result<Value, String> {
        let i = self.object_index(handle)?;
        let o = self.objects[i].as_ref().unwrap();
        let (p, r, s) = self.world_trs(handle)?;
        let (lp, lr, ls) = self.local_trs(handle, o)?;
        Ok(
            json!({"local_position":lp.to_array(),"local_rotation":lr.to_array(),"local_scale":ls.to_array(),"position":p.to_array(),"rotation":r.to_array(),"scale":s.to_array(),"parent":self.metadata[&handle].parent,"children":self.children(handle)}),
        )
    }
    pub(crate) fn set_trs(
        &mut self,
        handle: u64,
        position: Option<[f32; 3]>,
        rotation: Option<[f32; 4]>,
        scale: Option<[f32; 3]>,
        space: &str,
    ) -> Result<(), String> {
        let i = self.object_index(handle)?;
        if space != "local" && space != "world" {
            return Err("space must be local or world".into());
        }
        if position
            .iter()
            .flatten()
            .chain(scale.iter().flatten())
            .chain(rotation.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err("transform values must be finite".into());
        }
        if rotation.is_some_and(|r| Quat::from_array(r).length_squared() < 1e-12) {
            return Err("rotation quaternion must be nonzero".into());
        }
        let parent = if space == "world" {
            self.metadata[&handle]
                .parent
                .map(|p| self.world_trs(p))
                .transpose()?
        } else {
            None
        };
        if (position.is_some() || scale.is_some())
            && parent.is_some_and(|(_, _, scale)| scale.abs().min_element() < 1e-8)
        {
            return Err("world-space write requires invertible parent scale".into());
        }
        let o = self.objects[i].as_mut().unwrap();
        if let Some(p) = position {
            o.position = if let Some((pp, pr, ps)) = parent {
                (pr.inverse() * (Vec3::from(p) - pp) / ps).to_array()
            } else {
                p
            };
        }
        if let Some(r) = rotation {
            o.set_rotation(if let Some((_, pr, _)) = parent {
                pr.inverse() * Quat::from_array(r)
            } else {
                Quat::from_array(r)
            });
        }
        if let Some(s) = scale {
            o.scale = if let Some((_, _, ps)) = parent {
                (Vec3::from(s) / ps).to_array()
            } else {
                s
            };
        }
        let local_position = o.position;
        if position.is_some() {
            self.set_rect_position(handle, local_position)?;
        }
        Ok(())
    }
    pub(crate) fn parent(
        &mut self,
        object: u64,
        parent: Option<u64>,
        stays: bool,
    ) -> Result<(), String> {
        self.object_index(object)?;
        let mut current = parent;
        let mut depth = 0;
        while let Some(p) = current {
            self.object_index(p)?;
            if p == object {
                return Err("parent would create a cycle".into());
            }
            current = self.metadata[&p].parent;
            depth += 1;
            if depth >= 255 {
                return Err("hierarchy too deep".into());
            }
        }
        let trs = self.world_trs(object)?;
        self.metadata.get_mut(&object).unwrap().parent = parent;
        if stays {
            self.set_trs(
                object,
                Some(trs.0.to_array()),
                Some(trs.1.to_array()),
                Some(trs.2.to_array()),
                "world",
            )?;
        }
        Ok(())
    }
}
