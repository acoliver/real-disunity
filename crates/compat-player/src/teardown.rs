pub(crate) fn drain<T>(
    host: &mut T,
    managed: impl FnOnce(&mut T) -> Result<(), String>,
    native: impl FnOnce(&mut T) -> Result<(), String>,
) -> Result<(), String> {
    let managed = managed(host);
    let native = native(host);
    match (managed, native) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(managed), Err(native)) => Err(format!("{managed}; native cleanup: {native}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_cleanup_is_attempted_after_managed_disposal_failure() {
        let mut steps = Vec::new();
        let error = drain(
            &mut steps,
            |steps| {
                steps.push("managed");
                Err("original disposal error".into())
            },
            |steps| {
                steps.push("native");
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(steps, ["managed", "native"]);
        assert_eq!(error, "original disposal error");
    }

    #[test]
    fn independent_cleanup_errors_are_both_preserved() {
        let error = drain(
            &mut (),
            |()| Err("managed failure".into()),
            |()| Err("native failure".into()),
        )
        .unwrap_err();
        assert!(error.contains("managed failure"));
        assert!(error.contains("native failure"));
    }
}
