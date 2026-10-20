//! Source-level guard for the transparent overlay contract; live compositor
//! checks are still needed to verify mapping, keyboard focus, and presentation.

#[test]
fn picker_is_a_transparent_overlay_with_a_bounded_card() {
    let qml = include_str!("../Snipboard.qml");
    let (_, surface) = qml.split_once("    PanelWindow {").expect("layer-shell picker");
    let (window, card) = surface.split_once("        BorderSurface {").unwrap();
    assert!(window.contains("color: \"transparent\""));
    assert!(window.contains("WlrLayershell.namespace: \"omarchy-snipboard\""));
    assert!(window.contains("WlrKeyboardFocus.Exclusive"));
    assert!(window.contains("onClicked: root.close()"));
    assert!(!window.contains("Rectangle {"), "no screen-covering background");
    assert!(!qml.contains("Color.menu.scrim"));
    assert!(!qml.contains("FloatingWindow {"));
    assert!(card.contains("anchors.centerIn: parent"));
    assert!(card.contains("width: Math.min(Style.space(900), panel.width - Style.gapsOut * 2)"));
    assert!(card.contains("height: Math.min(Style.space(600), panel.height - Style.gapsOut * 2)"));
}
