use super::*;

fn plan(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_owned()).collect()
}

#[test]
fn the_reasons_a_material_has_no_shader_are_printed() {
    let differences = plan(&[
        "material Wall: references no shader; the engine's fallback shader is used",
        "material Water: shader references material object 7 of file 3, which did not plan: no such shader",
        "shader did not plan: shader object 12 of sharedassets0.assets: unsupported platform",
        "material Wall: 2 textures do not plan",
        "render texture did not plan: unsupported format",
        "compute shader did not plan: no kernels",
    ]);
    assert_eq!(
        shader_difference_lines(&differences),
        plan(&[
            "[player] shader: material Wall: references no shader; the engine's fallback shader is used",
            "[player] shader: material Water: shader references material object 7 of file 3, which did not plan: no such shader",
            "[player] shader: shader did not plan: shader object 12 of sharedassets0.assets: unsupported platform",
        ])
    );
}

#[test]
fn a_repeated_difference_is_printed_once() {
    let line = "material Wall: references no shader; the engine's fallback shader is used";
    let differences = plan(&[line, line, line]);
    assert_eq!(shader_difference_lines(&differences).len(), 1);
}

#[test]
fn nothing_is_printed_without_a_shader_difference() {
    assert!(shader_difference_lines(&plan(&["material Wall: 1 texture does not plan"])).is_empty());
    assert!(shader_difference_lines(&[]).is_empty());
}

#[test]
fn the_lines_are_capped_and_the_rest_counted() {
    let differences: Vec<String> = (0..45)
        .map(|index| {
            format!("material M{index}: references no shader; the engine's fallback shader is used")
        })
        .collect();
    let lines = shader_difference_lines(&differences);
    assert_eq!(lines.len(), 41);
    assert!(lines[0].contains("material M0:"));
    assert!(lines[39].contains("material M39:"));
    assert_eq!(lines[40], "[player] shader: and 5 more differences");
}
