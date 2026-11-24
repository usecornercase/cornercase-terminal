pub(super) const LUCIDE: &[(&str, &str)] = &[
    ("Box", "\u{f01a7}"),
    ("Boxes", "\u{f0b84}"),
    ("ClipboardList", "\u{f10d4}"),
    ("Code", "\u{eac4}"),
    ("DollarSign", "\u{f01c1}"),
    ("FlaskConical", "\u{f0093}"),
    ("Gamepad2", "\u{f0297}"),
    ("GraduationCap", "\u{f0474}"),
    ("LifeBuoy", "\u{f087e}"),
    ("Map", "\u{f034d}"),
    ("Megaphone", "\u{f00e6}"),
    ("PenTool", "\u{f0d13}"),
    ("Search", "\u{ea6d}"),
    ("ShieldCheck", "\u{f0565}"),
    ("Sparkles", "\u{f0674}"),
    ("Stethoscope", "\u{f04d9}"),
    ("Target", "\u{f08c9}"),
    ("TrendingUp", "\u{f0535}"),
];

pub(crate) fn lucide(name: &str) -> Option<&'static str> {
    let name = name.trim();
    LUCIDE.iter().find(|(known, _)| known.eq_ignore_ascii_case(name)).map(|(_, glyph)| *glyph)
}
