use super::{ApiSurface, AssemblyApiUse};

pub(super) fn keyboard_use(api: &AssemblyApiUse) -> bool {
    if api.assembly != "UnityCompat" {
        return false;
    }
    match api.surface {
        ApiSurface::KeyboardSnapshot => match api.type_name.as_str() {
            "UnityEngine.InputSystem.Keyboard" => matches!(
                api.member.as_str(),
                "get_current" | "get_wKey" | "get_aKey" | "get_sKey" | "get_dKey"
            ),
            "UnityEngine.InputSystem.KeyControl" => matches!(
                api.member.as_str(),
                "get_isPressed" | "get_wasPressedThisFrame" | "get_wasReleasedThisFrame"
            ),
            "UnityEngine.Input" => matches!(
                api.member.as_str(),
                "GetAxis" | "GetAxisRaw" | "GetKey" | "GetKeyDown" | "GetKeyUp"
            ),
            _ => false,
        },
        ApiSurface::CanonicalFacade => FACADE_MEMBERS.iter().any(|(name, members)| {
            api.type_name == *name && members.contains(&api.member.as_str())
        }),
        ApiSurface::InputActions | ApiSurface::Unknown => false,
    }
}

const FACADE_MEMBERS: &[(&str, &[&str])] = &[
    ("UnityEngine.Collision2D", &["get_gameObject"]),
    (
        "UnityEngine.Component",
        &[
            "CompareTag",
            "GetComponent",
            "get_gameObject",
            "get_tag",
            "get_transform",
            "set_enabled",
        ],
    ),
    ("UnityEngine.Debug", &["Log", "LogError"]),
    (
        "UnityEngine.GameObject",
        &[
            "CompareTag",
            "FindGameObjectWithTag",
            "GetComponent",
            "SetActive",
            "get_transform",
        ],
    ),
    ("UnityEngine.Mathf", &["Clamp", "Min"]),
    (
        "UnityEngine.MonoBehaviour",
        &[".ctor", "StartCoroutine", "StopCoroutine"],
    ),
    (
        "UnityEngine.Object",
        &[
            "Destroy",
            "FindFirstObjectByType",
            "FindObjectOfType",
            "FindObjectsByType",
            "Instantiate",
            "op_Equality",
            "op_Inequality",
        ],
    ),
    ("UnityEngine.Quaternion", &["get_identity"]),
    ("UnityEngine.Random", &["Range", "get_value"]),
    (
        "UnityEngine.Rigidbody2D",
        &[
            "MovePosition",
            "Sleep",
            "get_linearVelocity",
            "get_position",
            "set_angularVelocity",
            "set_linearVelocity",
            "set_velocity",
        ],
    ),
    ("UnityEngine.SpriteRenderer", &["set_sprite"]),
    ("UnityEngine.Time", &["get_deltaTime", "get_fixedDeltaTime"]),
    (
        "UnityEngine.Transform",
        &["get_localScale", "get_position", "set_position"],
    ),
    ("UnityEngine.UI.Text", &["get_text", "set_text"]),
    (
        "UnityEngine.Vector2",
        &[
            ".ctor",
            "get_normalized",
            "get_sqrMagnitude",
            "get_zero",
            "op_Implicit",
            "op_Multiply",
            "x",
            "y",
        ],
    ),
    (
        "UnityEngine.Vector3",
        &[".ctor", "op_Implicit", "op_Multiply", "x", "y"],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_facade_requires_a_grounded_type_member_pair() {
        for (member, expected) in [
            ("GetComponent", true),
            ("UnqualifiedReflectionDispatch", false),
        ] {
            let api = AssemblyApiUse {
                assembly: "UnityCompat".into(),
                type_name: "UnityEngine.GameObject".into(),
                member: member.into(),
                surface: ApiSurface::CanonicalFacade,
            };
            assert_eq!(keyboard_use(&api), expected);
        }
    }
}
