//! Sprint 26 — Country-interior mechanics pack (config-driven definition editor).
//!
//! One reusable core stamps the Sprint 20/21 editor shape (list → expand → typed
//! keys + 14.2 trigger/effect trees + `modifier` blocks + loc + create scaffold +
//! preserve-unknown raw) onto ~14 more `common/` registries. Each registry is a
//! [`Family`] in the [`FAMILIES`] config table — the single source of truth the
//! frontend mirrors (`src/lib/mechanics.ts`).
//!
//! # The config table
//! A `Family` declares: directory, project scaffold file, loc-key pattern
//! (`<key>` name / `desc_<key>` desc — verified across all Sprint-26 families),
//! typed scalars (with enum option sets), `modifier`-style flat blocks,
//! trigger/effect/weight script blocks (14.2 tree registries), whether the block
//! carries its modifiers **flat at the top level** (`self_modifier`, e.g.
//! personal deities / fetishist cults), whether it is an **ordered child list**
//! (`ordered`, religious reforms' `<x>_reform_N` steps), whether it is
//! **group-nested** (`group_nested`, religious schools live under
//! `<religion_group> = { religious_schools = { … } }` in `common/religions`), the
//! country-shaped **availability trigger** (14.3 evaluation), the scalar keys that
//! hold **event ids** (disasters/incidents `on_start`/`on_end`/`immediate_effect`
//! cross-ref), and a scaffold builder. Everything unmodeled round-trips untouched.
//!
//! # Editing model (existing typed-edit vocabulary only)
//! * Scalars → `SetScalar` (present) / `InsertStatement` (absent) at `[key, k]`.
//! * `modifier` flat blocks → typed `ModifierEditor` → `SetBlock`/`InsertStatement`
//!   (only when *flat*; nested content is shown read-only so a rewrite can't drop it).
//! * `self_modifier` rows → per-row `SetScalar`/`InsertStatement`/`RemoveStatement`
//!   at `[key, row]` (the block IS the modifier; a whole-block rewrite would drop
//!   the structural sub-blocks).
//! * Trigger/effect/weight blocks → the 14.2 `ScriptTreeEditor`.
//! * Ordered reform steps → per-step `ModifierEditor` + byte-surgical reorder.
//! * Loc name/desc → `LocOverride` on `<key>` / `desc_<key>`.

use crate::loc::{self, LocStore};
use crate::mod_writer;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

// ---------------------------------------------------------------------------
// Scalar / icon kinds.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ScalarKind {
    Num,
    Int,
    Bool,
    Enum,
    Str,
    /// A bare, unquoted identifier / date (`make_obsolete = fort_16th`,
    /// `historical_start_date = 1450.1.1`, `on_start = institution_events.2`).
    /// Distinct from `Str` (which round-trips quoted) so a picker/token value is
    /// written without quotes. Sprint 27.
    Token,
}

impl ScalarKind {
    fn as_str(self) -> &'static str {
        match self {
            ScalarKind::Num => "num",
            ScalarKind::Int => "int",
            ScalarKind::Bool => "bool",
            ScalarKind::Enum => "enum",
            ScalarKind::Str => "str",
            ScalarKind::Token => "token",
        }
    }
}

struct ScalarSpec {
    key: &'static str,
    kind: ScalarKind,
    options: &'static [&'static str],
    /// A registry/map picker the frontend renders instead of a plain input:
    /// "" (none) | "building" | "trade_good" | "province". Sprint 27 (config
    /// vocabulary growth): institutions' `historical_start_province` gets the
    /// on-map province picker, buildings' `make_obsolete` a building picker.
    picker: &'static str,
}

const fn s(key: &'static str, kind: ScalarKind) -> ScalarSpec {
    ScalarSpec { key, kind, options: &[], picker: "" }
}
const fn e(key: &'static str, options: &'static [&'static str]) -> ScalarSpec {
    ScalarSpec { key, kind: ScalarKind::Enum, options, picker: "" }
}
/// A scalar edited through a registry/map picker (Sprint 27).
const fn pk(key: &'static str, kind: ScalarKind, picker: &'static str) -> ScalarSpec {
    ScalarSpec { key, kind, options: &[], picker }
}

#[derive(Clone, Copy, PartialEq)]
enum IconKind {
    None,
    /// Positional sprite reference edited via the 14.4 sprite picker
    /// (`icon = GFX_holy_order_benedictines`).
    Sprite,
    /// A bare, quoted named-icon reference (`icon = "crown"`) resolved by the game
    /// to an interface sprite by name. Government reforms. Sprint 27.
    Named,
}

impl IconKind {
    fn as_str(self) -> &'static str {
        match self {
            IconKind::None => "none",
            IconKind::Sprite => "sprite",
            IconKind::Named => "named",
        }
    }
}

/// One bare-token list field (`manufactory = { grain fish }`). Sprint 27 config
/// vocabulary growth: buildings' manufactory trade-good list, editable with a
/// trade-goods picker. `picker` mirrors [`ScalarSpec::picker`].
struct ListSpec {
    name: &'static str,
    picker: &'static str,
}
const fn ls(name: &'static str, picker: &'static str) -> ListSpec {
    ListSpec { name, picker }
}

/// Named-sprite icon-strip emission on create (Sprint 27). Buildings and
/// institutions resolve their icon by *name* (`GFX_<key>` in
/// `interface/building_icons.gfx`; `GFX_icon_institution_<key>` in
/// `interface/countrytechnologyview.gfx`) — NOT a positional strip like trade
/// goods. So a created entity must ship a `spriteType` pointing at a real base
/// texture, or the game shows no icon. The scaffold emits a self-contained
/// `spriteTypes = { spriteType = { … } }` block appended to `gfx_file`.
struct IconGfx {
    /// Project-relative `.gfx` file the sprite is appended to.
    gfx_file: &'static str,
    /// Sprite name prefix; the sprite is `<prefix><key>`.
    sprite_prefix: &'static str,
    /// An existing base texture (double-slashed EU4 path) to point at.
    texture: &'static str,
}

/// One script block reference (trigger / effect / weight — the weight blocks use
/// the "triggers" registry for key suggestions, the tree preserves factor rows).
struct BlockSpec {
    name: &'static str,
    registry: &'static str, // "triggers" | "effects"
}
const fn t(name: &'static str) -> BlockSpec {
    BlockSpec { name, registry: "triggers" }
}
const fn f(name: &'static str) -> BlockSpec {
    BlockSpec { name, registry: "effects" }
}

/// A container of repeated same-shape child entries (Sprint 27 Wave 3: ages'
/// `objectives = { obj_x = { … } }` and `abilities = { ab_x = { … } }`). Each
/// child is modeled as a sub-entry, edited through the same typed-edit vocabulary
/// at the deeper path `[obj, container, child, …]`. `child_is_trigger` = the
/// whole child body is a trigger tree (age objectives); otherwise the child
/// carries its own flat modifier blocks + trigger/effect/weight scripts (age
/// abilities). Kept OUT of the `Family` struct (keyed by id via
/// [`family_sub_groups`]) to avoid churning the 25 existing family literals.
struct SubGroup {
    container: &'static str,
    label: &'static str,
    child_is_trigger: bool,
    child_modifiers: &'static [&'static str],
    child_scripts: &'static [BlockSpec],
    /// Minimal child scaffold body inserted by the "＋ add" affordance.
    child_scaffold: &'static str,
}

// ---------------------------------------------------------------------------
// Family config.
// ---------------------------------------------------------------------------

struct Family {
    id: &'static str,
    label: &'static str,
    dir: &'static str,
    project_file: &'static str,
    /// The single `common/religions` file schools scaffold into (group_nested).
    has_color: bool,
    icon: IconKind,
    scalars: &'static [ScalarSpec],
    modifiers: &'static [&'static str],
    /// Flat top-level modifiers (the block itself is the modifier list).
    self_modifier: bool,
    scripts: &'static [BlockSpec],
    /// Scalar keys whose values are event ids (jump to the events overlay).
    event_ref_keys: &'static [&'static str],
    /// Country-shaped availability trigger key ("" = none).
    avail_trigger: &'static str,
    /// Ordered child modifier blocks (religious reforms).
    ordered: bool,
    /// Nested under `<group> = { religious_schools = { … } }` in project_file.
    group_nested: bool,
    /// Bare-token list fields (buildings' `manufactory`). Sprint 27.
    list_fields: &'static [ListSpec],
    /// Named-sprite icon emission on create (buildings, institutions). Sprint 27.
    icon_gfx: Option<IconGfx>,
    /// Loc desc key pattern: `false` → `desc_<key>` (Sprint-26 families); `true`
    /// → `<key>_desc` (reforms/buildings/institutions, verified). Sprint 27.
    desc_suffix: bool,
    scaffold: fn(&str) -> String,
}

#[cfg(test)]
pub fn family_ids() -> Vec<&'static str> {
    FAMILIES.iter().map(|fam| fam.id).collect()
}

fn family_for(id: &str) -> Option<&'static Family> {
    FAMILIES.iter().find(|f| f.id == id)
}

// ---------------------------------------------------------------------------
// The 14 families.
// ---------------------------------------------------------------------------

static FAMILIES: &[Family] = &[
    Family {
        id: "disasters",
        label: "Disasters",
        dir: "common/disasters",
        project_file: "common/disasters/zz_eutoolkit_disasters.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("on_start", ScalarKind::Str), s("on_end", ScalarKind::Str)],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[t("potential"), t("can_start"), t("can_stop"), t("can_end"), t("progress")],
        event_ref_keys: &["on_start", "on_end"],
        avail_trigger: "can_start",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_disaster,
    },
    Family {
        id: "parliament_issues",
        label: "Parliament Issues",
        dir: "common/parliament_issues",
        project_file: "common/parliament_issues/zz_eutoolkit_parliament_issues.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("category", ScalarKind::Int)],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("allow"), f("effect"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "allow",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_parliament_issue,
    },
    Family {
        id: "parliament_bribes",
        label: "Parliament Bribes",
        dir: "common/parliament_bribes",
        project_file: "common/parliament_bribes/zz_eutoolkit_parliament_bribes.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("trigger"), f("effect"), t("chance"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_parliament_bribe,
    },
    Family {
        id: "factions",
        label: "Court Factions",
        dir: "common/factions",
        project_file: "common/factions/zz_eutoolkit_factions.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            e("monarch_power", &["ADM", "DIP", "MIL"]),
            s("always", ScalarKind::Bool),
        ],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_faction,
    },
    Family {
        id: "personal_deities",
        label: "Personal Deities",
        dir: "common/personal_deities",
        project_file: "common/personal_deities/zz_eutoolkit_personal_deities.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("sprite", ScalarKind::Int)],
        modifiers: &[],
        self_modifier: true,
        scripts: &[t("potential"), t("trigger"), f("effect"), f("removed_effect"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "potential",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_deity,
    },
    Family {
        id: "church_aspects",
        label: "Church Aspects",
        dir: "common/church_aspects",
        project_file: "common/church_aspects/zz_eutoolkit_church_aspects.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("cost", ScalarKind::Int)],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[f("effect"), t("allow"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "allow",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_church_aspect,
    },
    Family {
        id: "fetishist_cults",
        label: "Fetishist Cults",
        dir: "common/fetishist_cults",
        project_file: "common/fetishist_cults/zz_eutoolkit_fetishist_cults.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("sprite", ScalarKind::Int)],
        modifiers: &[],
        self_modifier: true,
        scripts: &[t("allow"), f("effect"), f("removed_effect"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "allow",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_cult,
    },
    Family {
        id: "holy_orders",
        label: "Holy Orders",
        dir: "common/holy_orders",
        project_file: "common/holy_orders/zz_eutoolkit_holy_orders.txt",
        has_color: true,
        icon: IconKind::Sprite,
        scalars: &[
            s("cost", ScalarKind::Int),
            e("cost_type", &["adm_power", "dip_power", "mil_power"]),
            s("localization", ScalarKind::Str),
        ],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[t("trigger"), f("per_province_effect"), f("per_province_abandon_effect"), t("ai_priority")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_holy_order,
    },
    Family {
        id: "fervor",
        label: "Fervor Aspects",
        dir: "common/fervor",
        project_file: "common/fervor/zz_eutoolkit_fervor.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("cost", ScalarKind::Int)],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("potential"), t("trigger"), f("effect"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "potential",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_fervor,
    },
    Family {
        id: "isolationism",
        label: "Isolationism Tiers",
        dir: "common/isolationism",
        project_file: "common/isolationism/zz_eutoolkit_isolationism.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("isolation_value", ScalarKind::Int)],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_isolationism,
    },
    Family {
        id: "golden_bulls",
        label: "Golden Bulls",
        dir: "common/golden_bulls",
        project_file: "common/golden_bulls/zz_eutoolkit_golden_bulls.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[t("potential"), t("trigger"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_golden_bull,
    },
    Family {
        id: "religious_reforms",
        label: "Religious Reforms",
        dir: "common/religious_reforms",
        project_file: "common/religious_reforms/zz_eutoolkit_religious_reforms.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("trigger"), t("can_buy_idea"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: true,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_religious_reform,
    },
    Family {
        id: "incidents",
        label: "Incidents (Shinto)",
        dir: "common/incidents",
        project_file: "common/incidents/zz_eutoolkit_incidents.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("frame", ScalarKind::Int), s("variable_initial", ScalarKind::Int)],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("potential"), t("trigger"), t("mean_time_to_happen"), f("immediate_effect")],
        event_ref_keys: &[],
        avail_trigger: "potential",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold:scaffold_incident,
    },
    Family {
        id: "religious_schools",
        label: "Religious Schools",
        dir: "common/religions",
        project_file: "common/religions/00_religion.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("picture", ScalarKind::Str),
            s("invite_scholar_modifier_display", ScalarKind::Str),
        ],
        modifiers: &[],
        self_modifier: true,
        scripts: &[t("potential_invite_scholar"), t("can_invite_scholar"), f("on_invite_scholar")],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: true,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_school,
    },
    // -----------------------------------------------------------------------
    // Sprint 27 — Wave 1: definition-editor sweep (government reforms,
    // buildings, institutions). These extend the config vocabulary: `Token`
    // scalars, registry/map `picker`s, bare-token `list_fields`, `Named` icons,
    // `icon_gfx` named-sprite emission, and `<key>_desc` loc.
    // -----------------------------------------------------------------------
    Family {
        id: "government_reforms",
        label: "Government Reforms",
        dir: "common/government_reforms",
        project_file: "common/government_reforms/zz_eutoolkit_government_reforms.txt",
        has_color: false,
        icon: IconKind::Named,
        scalars: &[
            s("nation_designer_cost", ScalarKind::Int),
            s("fixed_rank", ScalarKind::Int),
            s("duration", ScalarKind::Int),
            s("allow_normal_conversion", ScalarKind::Bool),
            s("legacy_government", ScalarKind::Bool),
            s("valid_for_new_country", ScalarKind::Bool),
            s("valid_for_nation_designer", ScalarKind::Bool),
            s("lock_level_when_selected", ScalarKind::Bool),
            s("monarchy", ScalarKind::Bool),
            s("republic", ScalarKind::Bool),
            s("theocracy", ScalarKind::Bool),
            s("tribal", ScalarKind::Bool),
            s("has_parliament", ScalarKind::Bool),
            s("royal_marriage", ScalarKind::Bool),
        ],
        modifiers: &["modifiers", "custom_attributes"],
        self_modifier: false,
        scripts: &[
            t("potential"),
            t("trigger"),
            t("nation_designer_trigger"),
            f("effect"),
            f("removed_effect"),
            t("ai"),
        ],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: true,
        scaffold: scaffold_government_reform,
    },
    Family {
        id: "buildings",
        label: "Buildings",
        dir: "common/buildings",
        project_file: "common/buildings/zz_eutoolkit_buildings.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("cost", ScalarKind::Int),
            s("time", ScalarKind::Int),
            pk("make_obsolete", ScalarKind::Token, "building"),
            s("onmap", ScalarKind::Bool),
            s("influencing_fort", ScalarKind::Bool),
            s("one_per_country", ScalarKind::Bool),
            s("show_separate", ScalarKind::Bool),
            s("allow_in_gold_provinces", ScalarKind::Bool),
        ],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[
            t("build_trigger"),
            t("keep_trigger"),
            t("potential"),
            f("on_built"),
            f("on_destroyed"),
            f("on_obsolete"),
            t("ai_will_do"),
        ],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[ls("manufactory", "trade_good")],
        icon_gfx: Some(IconGfx {
            gfx_file: "interface/zz_eutoolkit_buildings.gfx",
            sprite_prefix: "GFX_",
            texture: "gfx//interface//buildings//building_default.tga",
        }),
        desc_suffix: true,
        scaffold: scaffold_building,
    },
    Family {
        id: "institutions",
        label: "Institutions",
        dir: "common/institutions",
        project_file: "common/institutions/zz_eutoolkit_institutions.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("trade_company_efficiency", ScalarKind::Num),
            s("historical_start_date", ScalarKind::Token),
            pk("historical_start_province", ScalarKind::Int, "province"),
            s("start_chance", ScalarKind::Int),
            pk("on_start", ScalarKind::Token, ""),
        ],
        modifiers: &["bonus"],
        self_modifier: false,
        scripts: &[
            t("history"),
            t("can_start"),
            t("can_embrace"),
            t("embracement_speed"),
            t("ai_will_do"),
        ],
        event_ref_keys: &["on_start"],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: Some(IconGfx {
            gfx_file: "interface/zz_eutoolkit_institutions.gfx",
            sprite_prefix: "GFX_icon_institution_",
            texture: "gfx//interface//institutions//feudalism.dds",
        }),
        desc_suffix: true,
        scaffold: scaffold_institution,
    },
    // -----------------------------------------------------------------------
    // Sprint 27 — Wave 2: diplomacy & war. Governments + subject types (+ their
    // upgrades), CB / war-goal types (+ peace treaties), policies, power
    // projection. New config vocabulary: `subject_type` / `wargoal_type` /
    // `peace_treaty` picker kinds; loader-side per-family exclusions and the
    // subject-type forward-declaration de-dup (see `family_exclude` /
    // `family_dedup` / `edit_key`).
    // -----------------------------------------------------------------------
    Family {
        id: "governments",
        label: "Governments",
        dir: "common/governments",
        project_file: "common/governments/zz_eutoolkit_governments.txt",
        has_color: true,
        icon: IconKind::None,
        scalars: &[s("basic_reform", ScalarKind::Token)],
        modifiers: &[],
        self_modifier: false,
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        // `legacy_government` is a bare-token list of legacy reform keys; the
        // deep `reform_levels` / repeated `exclusive_reforms` blocks round-trip
        // as preserve-unknown.
        list_fields: &[ls("legacy_government", "")],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_government,
    },
    Family {
        id: "subject_types",
        label: "Subject Types",
        dir: "common/subject_types",
        project_file: "common/subject_types/zz_eutoolkit_subject_types.txt",
        has_color: false,
        icon: IconKind::None,
        // `copy_from` inherits another type; both are subject-type references.
        scalars: &[
            pk("copy_from", ScalarKind::Token, "subject_type"),
            s("relative_power_class", ScalarKind::Int),
            s("diplomacy_view_class", ScalarKind::Int),
            s("base_liberty_desire", ScalarKind::Int),
        ],
        // `modifier_subject` / `modifier_overlord` are flat modifier blocks (the
        // `= clear` scalar form, when present, is preserved untouched on save).
        modifiers: &["modifier_subject", "modifier_overlord"],
        // The ~50 remaining boolean/scalar properties are edited as flat rows.
        self_modifier: true,
        scripts: &[t("is_potential_overlord")],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_subject_type,
    },
    Family {
        id: "subject_type_upgrades",
        label: "Subject Type Upgrades",
        dir: "common/subject_type_upgrades",
        project_file: "common/subject_type_upgrades/zz_eutoolkit_subject_type_upgrades.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("cost", ScalarKind::Int)],
        modifiers: &["modifier_overlord", "modifier_subject"],
        self_modifier: false,
        scripts: &[t("can_upgrade_trigger"), f("effect"), f("removed_effect")],
        event_ref_keys: &[],
        avail_trigger: "can_upgrade_trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_subject_type_upgrade,
    },
    Family {
        id: "cb_types",
        label: "Casus Belli Types",
        dir: "common/cb_types",
        project_file: "common/cb_types/zz_eutoolkit_cb_types.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            pk("war_goal", ScalarKind::Token, "wargoal_type"),
            s("months", ScalarKind::Int),
            s("is_triggered_only", ScalarKind::Bool),
            s("valid_for_subject", ScalarKind::Bool),
            s("no_opinion_hit", ScalarKind::Bool),
        ],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("prerequisites"), t("prerequisites_self")],
        event_ref_keys: &[],
        avail_trigger: "prerequisites",
        ordered: false,
        group_nested: false,
        // `attacker_disabled_po` = a bare-token list of peace options to disable.
        list_fields: &[ls("attacker_disabled_po", "")],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_cb_type,
    },
    Family {
        id: "wargoal_types",
        label: "War Goal Types",
        dir: "common/wargoal_types",
        project_file: "common/wargoal_types/zz_eutoolkit_wargoal_types.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            e(
                "type",
                &[
                    "superiority",
                    "take_capital",
                    "take_province",
                    "take_core",
                    "take_border",
                    "take_colony",
                    "defend_capital",
                    "defend_country",
                    "naval_superiority",
                    "take_region",
                    "blockade_ports",
                ],
            ),
            s("war_name", ScalarKind::Token),
            s("prov_desc", ScalarKind::Token),
            s("country_desc", ScalarKind::Token),
            s("elector_relation", ScalarKind::Str),
            s("transfer_trade_cost_factor", ScalarKind::Num),
            s("allow_annex", ScalarKind::Bool),
            s("deny_annex", ScalarKind::Bool),
            s("allowed_provinces_are_eligible", ScalarKind::Bool),
            pk("required_treaty_to_take_provinces", ScalarKind::Token, "peace_treaty"),
        ],
        // attacker / defender carry the po_* peace options + factor scalars; they
        // are non-flat so the editor shows them read-only and round-trips them.
        modifiers: &["attacker", "defender"],
        self_modifier: false,
        scripts: &[t("allowed_provinces")],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_wargoal_type,
    },
    Family {
        id: "peace_treaties",
        label: "Peace Treaties",
        dir: "common/peace_treaties",
        project_file: "common/peace_treaties/zz_eutoolkit_peace_treaties.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("category", ScalarKind::Int),
            s("power_projection", ScalarKind::Token),
            s("power_cost_base", ScalarKind::Num),
            s("prestige_base", ScalarKind::Num),
            s("ae_base", ScalarKind::Num),
            s("warscore_cap", ScalarKind::Int),
            s("requires_demand_independence", ScalarKind::Bool),
            s("is_make_subject", ScalarKind::Bool),
            s("requires_is_allowed", ScalarKind::Bool),
            s("applies_to_war_target", ScalarKind::Bool),
        ],
        modifiers: &["warscore_cost"],
        self_modifier: false,
        scripts: &[t("is_visible"), t("is_allowed"), f("effect"), f("ai_weight")],
        event_ref_keys: &[],
        avail_trigger: "is_allowed",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: true,
        scaffold: scaffold_peace_treaty,
    },
    Family {
        id: "policies",
        label: "Policies",
        dir: "common/policies",
        project_file: "common/policies/zz_eutoolkit_policies.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[e("monarch_power", &["ADM", "DIP", "MIL"])],
        modifiers: &[],
        // The policy's modifiers are flat rows directly on the block.
        self_modifier: true,
        scripts: &[t("potential"), t("allow"), f("effect"), f("removed_effect"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "allow",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_policy,
    },
    Family {
        id: "powerprojection",
        label: "Power Projection",
        dir: "common/powerprojection",
        project_file: "common/powerprojection/zz_eutoolkit_powerprojection.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("power", ScalarKind::Int),
            s("max", ScalarKind::Int),
            s("min", ScalarKind::Int),
            s("decay", ScalarKind::Num),
            s("yearly_decay", ScalarKind::Num),
        ],
        modifiers: &[],
        self_modifier: false,
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_powerprojection,
    },
    // -----------------------------------------------------------------------
    // Sprint 27 — Wave 3: country mechanics. Generic idea groups, advisor
    // types, ruler/leader personalities, the four modifier registries
    // (event/opinion/static/triggered), ages (+ objectives/abilities sub-groups
    // + hegemons), government ranks (naming-table cross-link), state edicts,
    // and natives. Fits the config vocabulary as-is except ages' `sub_groups`
    // (kept out of the struct, keyed by id via `family_sub_groups`) and idea
    // groups' `category`-only inclusion filter (`family_filter`).
    // -----------------------------------------------------------------------
    Family {
        // The generic/pickable idea GROUPS (administrative_ideas, offensive_ideas,
        // …) — those declaring a `category`. National idea sets (`TAG_ideas`,
        // edited in the country panel's IdeasSection) have no category and are
        // excluded by `family_filter`, so the two editors never overlap.
        id: "idea_groups",
        label: "Idea Groups (generic)",
        dir: "common/ideas",
        project_file: "common/ideas/zz_eutoolkit_idea_groups.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[e("category", &["ADM", "DIP", "MIL"]), s("free", ScalarKind::Bool)],
        modifiers: &["start", "bonus"],
        self_modifier: false,
        scripts: &[t("trigger"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: true, // the group's individual ideas (child modifier blocks)
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: true,
        scaffold: scaffold_idea_group,
    },
    Family {
        id: "advisortypes",
        label: "Advisor Types",
        dir: "common/advisortypes",
        project_file: "common/advisortypes/zz_eutoolkit_advisortypes.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            e("monarch_power", &["ADM", "DIP", "MIL"]),
            s("allow_only_male", ScalarKind::Bool),
            s("allow_only_female", ScalarKind::Bool),
        ],
        modifiers: &[],
        // Flat modifier rows (prestige = 1, …). The repeated `skill_scaled_modifier`
        // blocks are non-flat → preserve-unknown, round-tripped untouched.
        self_modifier: true,
        scripts: &[t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_advisortype,
    },
    Family {
        id: "ruler_personalities",
        label: "Ruler Personalities",
        dir: "common/ruler_personalities",
        project_file: "common/ruler_personalities/zz_eutoolkit_ruler_personalities.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("nation_designer_cost", ScalarKind::Int)],
        modifiers: &[],
        self_modifier: true, // the personality's flat modifier rows
        // ruler/heir/consort allow blocks each wrap allow + chance; chance is a
        // weight. The 14.2 tree preserves the nested structure raw.
        scripts: &[t("ruler_allow"), t("heir_allow"), t("consort_allow"), t("chance")],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_ruler_personality,
    },
    Family {
        id: "leader_personalities",
        label: "Leader Personalities",
        dir: "common/leader_personalities",
        project_file: "common/leader_personalities/zz_eutoolkit_leader_personalities.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: true, // flat modifier rows applied to the led stack
        scripts: &[t("allow")],
        event_ref_keys: &[],
        avail_trigger: "allow",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_leader_personality,
    },
    Family {
        id: "event_modifiers",
        label: "Event Modifiers",
        dir: "common/event_modifiers",
        project_file: "common/event_modifiers/zz_eutoolkit_event_modifiers.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: true, // the block IS a flat modifier list
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_flat_modifier,
    },
    Family {
        id: "opinion_modifiers",
        label: "Opinion Modifiers",
        dir: "common/opinion_modifiers",
        project_file: "common/opinion_modifiers/zz_eutoolkit_opinion_modifiers.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: true, // opinion/max/min/decay/months flat rows
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_opinion_modifier,
    },
    Family {
        id: "static_modifiers",
        label: "Static Modifiers",
        dir: "common/static_modifiers",
        project_file: "common/static_modifiers/zz_eutoolkit_static_modifiers.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: true, // the block IS a flat modifier list
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_flat_modifier,
    },
    Family {
        id: "triggered_modifiers",
        label: "Triggered Modifiers",
        dir: "common/triggered_modifiers",
        project_file: "common/triggered_modifiers/zz_eutoolkit_triggered_modifiers.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: true, // flat modifier rows applied when trigger holds
        scripts: &[t("potential"), t("trigger")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_triggered_modifier,
    },
    Family {
        // Ages carry two sub-entry containers (objectives + abilities); see
        // `family_sub_groups`. `absolutism` is a flat modifier block present only
        // on age_of_absolutism.
        id: "ages",
        label: "Ages",
        dir: "common/ages",
        project_file: "common/ages/zz_eutoolkit_ages.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("start", ScalarKind::Int),
            s("papacy", ScalarKind::Num),
            s("religious_conflicts", ScalarKind::Bool),
        ],
        modifiers: &["absolutism"],
        self_modifier: false,
        scripts: &[t("can_start")],
        event_ref_keys: &[],
        avail_trigger: "can_start",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_age,
    },
    Family {
        id: "hegemons",
        label: "Hegemons",
        dir: "common/hegemons",
        project_file: "common/hegemons/zz_eutoolkit_hegemons.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &["base", "scale", "max"],
        self_modifier: false,
        scripts: &[t("allow"), f("effect"), f("removed_effect")],
        event_ref_keys: &[],
        avail_trigger: "allow",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_hegemon,
    },
    Family {
        // Government ranks are keyed by rank NUMBER (`2 = { … }`). The rank editor
        // cross-links to the government-names editor (ranks index the naming
        // tables' `1=LOC 2=LOC 3=LOC` cells) via the frontend `onopennaming`.
        id: "government_ranks",
        label: "Government Ranks",
        dir: "common/government_ranks",
        project_file: "common/government_ranks/zz_eutoolkit_government_ranks.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: true, // per-rank flat modifier rows
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_government_rank,
    },
    Family {
        id: "state_edicts",
        label: "State Edicts",
        dir: "common/state_edicts",
        project_file: "common/state_edicts/zz_eutoolkit_state_edicts.txt",
        has_color: true,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[t("potential"), t("allow"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "allow",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_state_edict,
    },
    Family {
        // "Native advancement" — the target directory `common/native_advancement`
        // does not exist in this EU4 version; `common/natives` is the actual
        // native-settlement definitions (graphical culture / colour / icon / unit
        // + a province membership list).
        id: "natives",
        label: "Natives",
        dir: "common/natives",
        project_file: "common/natives/zz_eutoolkit_natives.txt",
        has_color: true,
        icon: IconKind::None,
        scalars: &[
            s("graphical_culture", ScalarKind::Token),
            s("unit", ScalarKind::Token),
            s("icon", ScalarKind::Int),
        ],
        modifiers: &[],
        self_modifier: false,
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[ls("provinces", "province")],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_natives,
    },
    // -----------------------------------------------------------------------
    // Sprint 27 — Wave 4: trade & military. Trading policies (trade-node
    // panel), trade-company investments (19.2 ColonialPanel), center-of-trade
    // tiers (province CoT control), naval doctrines, army-professionalism tiers,
    // flagship modifications. No new config vocabulary — reuses `Token` scalars,
    // flat modifier blocks, weight scripts (the `triggers` registry preserves
    // factor rows), and `self_modifier` flat rows. Directory names verified
    // against the install: `common/professionalism` (NOT army_professionalism),
    // `common/flagship_modifications`.
    // -----------------------------------------------------------------------
    Family {
        id: "trading_policies",
        label: "Trading Policies",
        dir: "common/trading_policies",
        project_file: "common/trading_policies/zz_eutoolkit_trading_policies.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("button_gfx", ScalarKind::Token),
            s("center_of_reformation", ScalarKind::Bool),
            s("cooldown", ScalarKind::Bool),
            s("unique", ScalarKind::Bool),
            s("show_alert", ScalarKind::Bool),
        ],
        // `trade_power = { duration power_modifier key }` is a flat named block;
        // `node_province_modifier` / `countries_with_merchant_modifier` are flat
        // province/country modifier blocks.
        modifiers: &["trade_power", "node_province_modifier", "countries_with_merchant_modifier"],
        self_modifier: false,
        scripts: &[t("potential"), t("can_select"), t("can_maintain")],
        event_ref_keys: &[],
        avail_trigger: "can_select",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_trading_policy,
    },
    Family {
        id: "tradecompany_investments",
        label: "Trade Company Investments",
        dir: "common/tradecompany_investments",
        project_file: "common/tradecompany_investments/zz_eutoolkit_tradecompany_investments.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("category", ScalarKind::Token),
            s("cost", ScalarKind::Num),
            s("upgrades_to", ScalarKind::Token),
            // `sprite = "GFX_investment_…"` is a quoted named-sprite reference.
            s("sprite", ScalarKind::Str),
        ],
        modifiers: &[
            "company_province_area_modifier",
            "area_modifier",
            "company_region_modifier",
            "owner_modifier",
            "owner_company_region_modifier",
        ],
        self_modifier: false,
        // ai_*_worth are weight blocks (factor + modifier rows).
        scripts: &[t("allow"), t("ai_global_worth"), t("ai_area_worth"), t("ai_region_worth")],
        event_ref_keys: &[],
        avail_trigger: "allow",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_tradecompany_investment,
    },
    Family {
        id: "centers_of_trade",
        label: "Centers of Trade",
        dir: "common/centers_of_trade",
        project_file: "common/centers_of_trade/zz_eutoolkit_centers_of_trade.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("level", ScalarKind::Int),
            s("development", ScalarKind::Int),
            s("cost", ScalarKind::Int),
            e("type", &["inland", "coastal"]),
        ],
        modifiers: &["province_modifiers", "state_modifiers", "global_modifiers"],
        self_modifier: false,
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_center_of_trade,
    },
    Family {
        id: "naval_doctrines",
        label: "Naval Doctrines",
        dir: "common/naval_doctrines",
        project_file: "common/naval_doctrines/zz_eutoolkit_naval_doctrines.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("cost", ScalarKind::Num), s("button_gfx", ScalarKind::Int)],
        modifiers: &["country_modifier"],
        self_modifier: false,
        scripts: &[t("can_select"), f("effect"), f("removed_effect")],
        event_ref_keys: &[],
        avail_trigger: "can_select",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_naval_doctrine,
    },
    Family {
        // `common/professionalism` (NOT army_professionalism). Each tier gates on
        // an `army_professionalism` threshold; the `may_*` ability grants + drill /
        // general-cost modifiers are flat rows (`self_modifier`).
        id: "professionalism",
        label: "Army Professionalism",
        dir: "common/professionalism",
        project_file: "common/professionalism/zz_eutoolkit_professionalism.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("army_professionalism", ScalarKind::Num),
            s("hidden", ScalarKind::Bool),
            s("marker_sprite", ScalarKind::Token),
            s("unit_sprite_start", ScalarKind::Str),
        ],
        modifiers: &[],
        self_modifier: true,
        scripts: &[t("trigger")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_professionalism,
    },
    Family {
        // Flagship mods have NO top-level cost key — costs are `*_cost_*` rows
        // inside the `modifier` block. `ai_trade_score` / `ai_war_score` weights.
        id: "flagship_modifications",
        label: "Flagship Modifications",
        dir: "common/flagship_modifications",
        project_file: "common/flagship_modifications/zz_eutoolkit_flagship_modifications.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("base_modification", ScalarKind::Bool)],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[t("trigger"), t("ai_trade_score"), t("ai_war_score")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_flagship_modification,
    },
    // -----------------------------------------------------------------------
    // Sprint 27 — Wave 5: flavor. Custom ideas (nation designer), AI
    // personalities, insults. Skipped-and-reported: ai_attitudes (hardcoded —
    // the vanilla file is all comments, zero entries), compliments (no such
    // directory in this EU4 version), custom_country_colors / dynasty_colors
    // (non-keyed color-pool registries — repeated `color = {…}` with no
    // per-entry key / loc / scaffold, so they don't fit the keyed-entry model).
    // AI personalities carry `family_no_create` (the game forbids new names).
    // -----------------------------------------------------------------------
    Family {
        // Nation-designer custom-idea CATEGORIES (adm_idea_modifiers, …), keyed by
        // `category` (ADM/DIP/MIL). The per-idea children (`custom_idea_*`) are
        // ordered child blocks carrying a flat modifier + `level_cost_N` /
        // `max_level` / `default` rows; each idea's `chance` weight round-trips raw.
        id: "custom_ideas",
        label: "Custom Ideas (Designer)",
        dir: "common/custom_ideas",
        project_file: "common/custom_ideas/zz_eutoolkit_custom_ideas.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[e("category", &["ADM", "DIP", "MIL"])],
        modifiers: &[],
        self_modifier: false,
        scripts: &[],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: true,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_custom_ideas,
    },
    Family {
        // Creation is disabled (`family_no_create`): the vanilla file states
        // "New personalities cannot be added". The `chance` weight + `icon` frame
        // are typed; everything else (befriend/rival/interesting-country weight
        // blocks) round-trips raw.
        id: "ai_personalities",
        label: "AI Personalities",
        dir: "common/ai_personalities",
        project_file: "common/ai_personalities/zz_eutoolkit_ai_personalities.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("icon", ScalarKind::Int)],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("chance")],
        event_ref_keys: &[],
        avail_trigger: "",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_ai_personality,
    },
    Family {
        id: "insults",
        label: "Insults",
        dir: "common/insults",
        project_file: "common/insults/zz_eutoolkit_insults.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("trigger")],
        event_ref_keys: &[],
        avail_trigger: "trigger",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: false,
        scaffold: scaffold_insult,
    },
    // ---- Sprint 29: Empires (HRE + Mandate). These three live behind the
    // View ▸ Empires overlay, not the Mechanics family selector (`family_hidden`
    // filters them out of `get_mechanic_families`), but reuse the whole
    // config-driven object editor. Imperial reforms carry SEVERAL flat modifier
    // blocks gated by scope (emperor / member / elector / all / province /
    // emperor_per_prince); `empire` (hre|celestial_empire) filters the tab and
    // `required_reform` is the chain link (Token → rendered as a jump link).
    Family {
        id: "imperial_reforms",
        label: "Imperial Reforms",
        dir: "common/imperial_reforms",
        project_file: "common/imperial_reforms/zz_eutoolkit_imperial_reforms.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[
            s("empire", ScalarKind::Token),
            s("required_reform", ScalarKind::Token),
            s("gui_container", ScalarKind::Token),
        ],
        modifiers: &["emperor", "member", "elector", "all", "province", "emperor_per_prince"],
        self_modifier: false,
        scripts: &[t("potential"), t("trigger"), f("on_effect"), f("off_effect")],
        event_ref_keys: &[],
        avail_trigger: "potential",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: true,
        scaffold: scaffold_imperial_reform_min,
    },
    Family {
        id: "imperial_incidents",
        label: "Imperial Incidents",
        dir: "common/imperial_incidents",
        project_file: "common/imperial_incidents/zz_eutoolkit_imperial_incidents.txt",
        has_color: false,
        icon: IconKind::None,
        // `event` is the incident's driving event id; `default_option` the 0-based
        // fallback. The numbered `0/1/2 = { … }` AI-weight option blocks are
        // preserve-unknown (surfaced editable in the Empires overlay via the raw
        // block; they don't fit fixed script-block names).
        scalars: &[s("event", ScalarKind::Token), s("default_option", ScalarKind::Int)],
        modifiers: &[],
        self_modifier: false,
        scripts: &[t("can_stop")],
        event_ref_keys: &["event"],
        avail_trigger: "can_stop",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: true,
        scaffold: scaffold_imperial_incident,
    },
    Family {
        id: "decrees",
        label: "Decrees",
        dir: "common/decrees",
        project_file: "common/decrees/zz_eutoolkit_decrees.txt",
        has_color: false,
        icon: IconKind::None,
        scalars: &[s("cost", ScalarKind::Int), s("duration", ScalarKind::Int)],
        modifiers: &["modifier"],
        self_modifier: false,
        scripts: &[t("potential"), t("trigger"), f("effect"), f("removed_effect"), t("ai_will_do")],
        event_ref_keys: &[],
        avail_trigger: "potential",
        ordered: false,
        group_nested: false,
        list_fields: &[],
        icon_gfx: None,
        desc_suffix: true,
        scaffold: scaffold_decree,
    },
];

// ---------------------------------------------------------------------------
// Payload types (serialize camelCase; mirrored by src/lib/mechanics.ts).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct ModRow {
    pub key: String,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModifierBlock {
    pub name: String,
    pub present: bool,
    pub flat: bool,
    pub rows: Vec<ModRow>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScriptBlockRef {
    pub name: String,
    pub registry: String,
    pub present: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scalar {
    pub key: String,
    pub kind: String,
    pub present: bool,
    pub value: String,
    pub options: Vec<String>,
    /// Registry/map picker: "" | "building" | "trade_good" | "province" (Sprint 27).
    pub picker: String,
}

/// One bare-token list field (`manufactory = { grain fish }`). Sprint 27.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListField {
    pub name: String,
    pub present: bool,
    /// Picker for its tokens ("trade_good", …).
    pub picker: String,
    pub tokens: Vec<String>,
}

/// One ordered child modifier block (religious reforms' `<x>_reform_N`).
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReformStep {
    pub key: String,
    pub flat: bool,
    pub rows: Vec<ModRow>,
}

/// One child of a sub-group container (an age objective or ability). Sprint 27 W3.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubEntry {
    pub key: String,
    pub name: String,
    /// Empty when the sub-group's child is a whole-body trigger (objectives).
    pub modifier_blocks: Vec<ModifierBlock>,
    pub script_blocks: Vec<ScriptBlockRef>,
    pub raw_extra: Vec<String>,
    pub raw: String,
}

/// A container of repeated same-shape children (ages' objectives / abilities).
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubGroupData {
    pub container: String,
    pub label: String,
    /// Whole child body is a trigger tree (objectives).
    pub child_is_trigger: bool,
    /// Flat modifier block names each ability child carries.
    pub child_modifiers: Vec<String>,
    /// Script blocks each ability child carries (name + registry).
    pub child_scripts: Vec<ScriptBlockRef>,
    pub entries: Vec<SubEntry>,
    /// Minimal child body the "＋ add" affordance inserts.
    pub child_scaffold: String,
}

/// A scalar whose value is an event id, for the "linked events" cross-reference.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EventRef {
    pub key: String,
    pub id: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MechanicObject {
    pub family: String,
    pub key: String,
    /// The occurrence-qualified path segment the editor uses for byte-surgical
    /// edits (`key` normally; `key#n` for a de-duplicated family whose real
    /// definition is the nth block of that key — subject types forward-declare
    /// every type as `key = {}` then define it as `key = { … }`). Sprint 27 W2.
    pub edit_key: String,
    pub file: String,
    pub origin: String,
    pub name: String,
    pub name_key: String,
    pub desc_key: String,
    pub desc_loc: Option<String>,
    pub icon: Option<String>,
    pub icon_kind: String,
    pub color: Option<[u8; 3]>,
    pub scalars: Vec<Scalar>,
    pub modifier_blocks: Vec<ModifierBlock>,
    /// Bare-token list fields (buildings' manufactory). Sprint 27.
    pub list_fields: Vec<ListField>,
    /// Present only when the family is `self_modifier`.
    pub self_modifier: bool,
    pub self_rows: Vec<ModRow>,
    pub script_blocks: Vec<ScriptBlockRef>,
    /// Present only when the family is `ordered`.
    pub ordered: bool,
    pub ordered_children: Vec<ReformStep>,
    /// Sub-entry containers (ages' objectives / abilities). Empty for others.
    pub sub_groups: Vec<SubGroupData>,
    pub event_refs: Vec<EventRef>,
    /// For group-nested families: the owning religion group key (edit path prefix).
    pub group: Option<String>,
    pub raw_extra: Vec<String>,
    pub raw: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FamilyMeta {
    pub id: String,
    pub label: String,
    /// The family's `common/…` source directory (for project-wide-search
    /// path→editor routing, Sprint 30.3).
    pub dir: String,
    pub project_file: String,
    pub has_color: bool,
    pub icon_kind: String,
    pub self_modifier: bool,
    pub ordered: bool,
    pub group_nested: bool,
    /// "" when the family has no country-shaped availability trigger.
    pub avail_trigger: String,
    /// The known script-block names + registries (for created-object rendering).
    pub script_blocks: Vec<ScriptBlockRef>,
    pub scalars: Vec<Scalar>,
    pub modifiers: Vec<String>,
    /// Bare-token list fields (name + picker), Sprint 27.
    pub list_fields: Vec<ListField>,
    /// Whether creating an entity emits a named-sprite `.gfx` entry (Sprint 27).
    pub icon_gfx: bool,
    /// Loc desc key pattern: false → `desc_<key>`; true → `<key>_desc`. Sprint 27.
    pub desc_suffix: bool,
    /// Whether the frontend offers a "＋ new…" create affordance (false for
    /// families the game hardcodes, e.g. AI personalities). Sprint 27 W5.
    pub allow_create: bool,
    /// Sub-entry container specs (ages' objectives/abilities) for rendering
    /// freshly-scaffolded objects the backend hasn't re-parsed. Sprint 27 W3.
    pub sub_groups: Vec<SubGroupData>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MechanicsData {
    pub meta: FamilyMeta,
    pub objects: Vec<MechanicObject>,
}

// ---------------------------------------------------------------------------
// Parse.
// ---------------------------------------------------------------------------

fn origin_of(vfs: &Vfs, path: &std::path::Path) -> &'static str {
    if vfs.mod_dir().is_some_and(|m| path.starts_with(m)) {
        "mod"
    } else {
        "base"
    }
}

fn modifier_block(b: &Block, name: &str) -> ModifierBlock {
    match b.get_block(name) {
        None => ModifierBlock { name: name.to_string(), present: false, flat: true, rows: Vec::new() },
        Some(inner) => {
            let mut rows = Vec::new();
            let mut flat = true;
            for (k, v) in &inner.items {
                match (k, v) {
                    (Some(k), Value::Scalar(sv)) => rows.push(ModRow { key: k.clone(), value: sv.clone() }),
                    _ => flat = false,
                }
            }
            ModifierBlock { name: name.to_string(), present: true, flat, rows }
        }
    }
}

/// The set of top-level keys the family models structurally (everything else is
/// preserve-unknown, or — when `self_modifier` — a flat modifier row).
fn structural_keys(fam: &Family) -> std::collections::HashSet<&'static str> {
    let mut set = std::collections::HashSet::new();
    if fam.icon != IconKind::None {
        set.insert("icon");
    }
    if fam.has_color {
        set.insert("color");
    }
    for sc in fam.scalars {
        set.insert(sc.key);
    }
    for m in fam.modifiers {
        set.insert(*m);
    }
    for b in fam.scripts {
        set.insert(b.name);
    }
    for l in fam.list_fields {
        set.insert(l.name);
    }
    for sg in family_sub_groups(fam) {
        set.insert(sg.container);
    }
    set
}

fn parse_object(
    file_bytes: &[u8],
    key: &str,
    edit_key: &str,
    b: &Block,
    loc: &LocStore,
    file: &str,
    origin: &str,
    fam: &Family,
    group: Option<&str>,
) -> MechanicObject {
    let icon = if fam.icon != IconKind::None {
        b.get_scalar("icon").map(|s| s.trim().to_string())
    } else {
        None
    };
    let color = if fam.has_color {
        b.get_block("color").and_then(paradox::color_from_block)
    } else {
        None
    };

    let scalars = fam
        .scalars
        .iter()
        .map(|spec| {
            let val = b.get_scalar(spec.key).map(|s| s.trim().to_string());
            Scalar {
                key: spec.key.to_string(),
                kind: spec.kind.as_str().to_string(),
                present: val.is_some(),
                value: val.unwrap_or_default(),
                options: spec.options.iter().map(|o| o.to_string()).collect(),
                picker: spec.picker.to_string(),
            }
        })
        .collect();

    let modifier_blocks = fam.modifiers.iter().map(|name| modifier_block(b, name)).collect();

    // Bare-token list fields (buildings' manufactory).
    let list_fields: Vec<ListField> = fam
        .list_fields
        .iter()
        .map(|spec| match b.get_block(spec.name) {
            None => ListField {
                name: spec.name.to_string(),
                present: false,
                picker: spec.picker.to_string(),
                tokens: Vec::new(),
            },
            Some(inner) => ListField {
                name: spec.name.to_string(),
                present: true,
                picker: spec.picker.to_string(),
                tokens: inner.bare_scalars().map(|s| s.to_string()).collect(),
            },
        })
        .collect();

    let script_blocks: Vec<ScriptBlockRef> = fam
        .scripts
        .iter()
        .map(|spec| ScriptBlockRef {
            name: spec.name.to_string(),
            registry: spec.registry.to_string(),
            present: b.get_block(spec.name).is_some(),
        })
        .collect();

    let structural = structural_keys(fam);

    // Self-modifier rows: top-level `k = scalar` not in the structural set.
    let mut self_rows: Vec<ModRow> = Vec::new();
    if fam.self_modifier {
        for (k, v) in &b.items {
            if let (Some(k), Value::Scalar(sv)) = (k.as_deref(), v) {
                if !structural.contains(k) {
                    self_rows.push(ModRow { key: k.to_string(), value: sv.clone() });
                }
            }
        }
    }

    // Ordered children: every child block whose key isn't structural.
    let mut ordered_children: Vec<ReformStep> = Vec::new();
    if fam.ordered {
        for (k, v) in &b.items {
            if let (Some(k), Value::Block(cb)) = (k.as_deref(), v) {
                if structural.contains(k) {
                    continue;
                }
                let mut rows = Vec::new();
                let mut flat = true;
                for (ik, iv) in &cb.items {
                    match (ik, iv) {
                        (Some(ik), Value::Scalar(sv)) => rows.push(ModRow { key: ik.clone(), value: sv.clone() }),
                        _ => flat = false,
                    }
                }
                ordered_children.push(ReformStep { key: k.to_string(), flat, rows });
            }
        }
    }

    let span_path: Vec<String> = match group {
        Some(g) => vec![g.to_string(), "religious_schools".to_string(), edit_key.to_string()],
        None => vec![edit_key.to_string()],
    };

    // Sub-entry containers (ages' objectives / abilities).
    let mut sub_groups: Vec<SubGroupData> = Vec::new();
    for sg in family_sub_groups(fam) {
        let mut entries: Vec<SubEntry> = Vec::new();
        if let Some(container) = b.get_block(sg.container) {
            for (child_key, cb) in container.key_blocks() {
                let modifier_blocks: Vec<ModifierBlock> = if sg.child_is_trigger {
                    Vec::new()
                } else {
                    sg.child_modifiers.iter().map(|n| modifier_block(cb, n)).collect()
                };
                let script_blocks: Vec<ScriptBlockRef> = if sg.child_is_trigger {
                    Vec::new()
                } else {
                    sg.child_scripts
                        .iter()
                        .map(|s| ScriptBlockRef {
                            name: s.name.to_string(),
                            registry: s.registry.to_string(),
                            present: cb.get_block(s.name).is_some(),
                        })
                        .collect()
                };
                // Preserve-unknown for ability children: keys not modeled here.
                let modeled: std::collections::HashSet<&str> = sg
                    .child_modifiers
                    .iter()
                    .copied()
                    .chain(sg.child_scripts.iter().map(|s| s.name))
                    .collect();
                let mut child_raw_extra: Vec<String> = Vec::new();
                if !sg.child_is_trigger {
                    let mut seen = std::collections::HashSet::new();
                    for (k, _) in &cb.items {
                        if let Some(k) = k.as_deref() {
                            if !modeled.contains(k) && seen.insert(k.to_string()) {
                                child_raw_extra.push(k.to_string());
                            }
                        }
                    }
                }
                let mut child_path = span_path.clone();
                child_path.push(sg.container.to_string());
                child_path.push(child_key.to_string());
                let raw = mod_writer::block_span(file_bytes, &child_path)
                    .map(|(s, e)| String::from_utf8_lossy(&file_bytes[s..e]).into_owned())
                    .unwrap_or_default();
                entries.push(SubEntry {
                    key: child_key.to_string(),
                    name: loc.resolve(child_key),
                    modifier_blocks,
                    script_blocks,
                    raw_extra: child_raw_extra,
                    raw,
                });
            }
        }
        sub_groups.push(SubGroupData {
            container: sg.container.to_string(),
            label: sg.label.to_string(),
            child_is_trigger: sg.child_is_trigger,
            child_modifiers: sg.child_modifiers.iter().map(|s| s.to_string()).collect(),
            child_scripts: sg
                .child_scripts
                .iter()
                .map(|s| ScriptBlockRef { name: s.name.to_string(), registry: s.registry.to_string(), present: false })
                .collect(),
            entries,
            child_scaffold: sg.child_scaffold.to_string(),
        });
    }

    // Event references (disasters/incidents on_start/on_end event ids).
    let mut event_refs: Vec<EventRef> = Vec::new();
    for k in fam.event_ref_keys {
        if let Some(v) = b.get_scalar(k) {
            let id = v.trim().trim_matches('"').to_string();
            if !id.is_empty() {
                event_refs.push(EventRef { key: k.to_string(), id });
            }
        }
    }

    // Preserve-unknown: top-level keys not modeled and (when self_modifier) not a
    // flat modifier row and (when ordered) not an ordered child.
    let self_row_keys: std::collections::HashSet<&str> = self_rows.iter().map(|r| r.key.as_str()).collect();
    let child_keys: std::collections::HashSet<&str> = ordered_children.iter().map(|r| r.key.as_str()).collect();
    let mut raw_extra: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (k, _) in &b.items {
        if let Some(k) = k.as_deref() {
            if !structural.contains(k)
                && !self_row_keys.contains(k)
                && !child_keys.contains(k)
                && seen.insert(k.to_string())
            {
                raw_extra.push(k.to_string());
            }
        }
    }

    let raw = mod_writer::block_span(file_bytes, &span_path)
        .map(|(s, e)| String::from_utf8_lossy(&file_bytes[s..e]).into_owned())
        .unwrap_or_default();

    let desc_key = desc_key_of(fam, key);
    MechanicObject {
        family: fam.id.to_string(),
        key: key.to_string(),
        edit_key: edit_key.to_string(),
        file: file.to_string(),
        origin: origin.to_string(),
        name: loc.resolve(key),
        name_key: key.to_string(),
        desc_loc: loc.get(&desc_key).map(str::to_string),
        desc_key,
        icon,
        icon_kind: fam.icon.as_str().to_string(),
        color,
        scalars,
        modifier_blocks,
        list_fields,
        self_modifier: fam.self_modifier,
        self_rows,
        script_blocks,
        ordered: fam.ordered,
        ordered_children,
        sub_groups,
        event_refs,
        group: group.map(str::to_string),
        raw_extra,
        raw,
    }
}

fn family_meta(fam: &Family) -> FamilyMeta {
    FamilyMeta {
        id: fam.id.to_string(),
        label: fam.label.to_string(),
        dir: fam.dir.to_string(),
        project_file: fam.project_file.to_string(),
        has_color: fam.has_color,
        icon_kind: fam.icon.as_str().to_string(),
        self_modifier: fam.self_modifier,
        ordered: fam.ordered,
        group_nested: fam.group_nested,
        avail_trigger: fam.avail_trigger.to_string(),
        script_blocks: fam
            .scripts
            .iter()
            .map(|b| ScriptBlockRef { name: b.name.to_string(), registry: b.registry.to_string(), present: false })
            .collect(),
        scalars: fam
            .scalars
            .iter()
            .map(|spec| Scalar {
                key: spec.key.to_string(),
                kind: spec.kind.as_str().to_string(),
                present: false,
                value: String::new(),
                options: spec.options.iter().map(|o| o.to_string()).collect(),
                picker: spec.picker.to_string(),
            })
            .collect(),
        modifiers: fam.modifiers.iter().map(|m| m.to_string()).collect(),
        list_fields: fam
            .list_fields
            .iter()
            .map(|spec| ListField {
                name: spec.name.to_string(),
                present: false,
                picker: spec.picker.to_string(),
                tokens: Vec::new(),
            })
            .collect(),
        icon_gfx: fam.icon_gfx.is_some(),
        desc_suffix: fam.desc_suffix,
        allow_create: !family_no_create(fam),
        sub_groups: family_sub_groups(fam)
            .iter()
            .map(|sg| SubGroupData {
                container: sg.container.to_string(),
                label: sg.label.to_string(),
                child_is_trigger: sg.child_is_trigger,
                child_modifiers: sg.child_modifiers.iter().map(|s| s.to_string()).collect(),
                child_scripts: sg
                    .child_scripts
                    .iter()
                    .map(|s| ScriptBlockRef { name: s.name.to_string(), registry: s.registry.to_string(), present: false })
                    .collect(),
                entries: Vec::new(),
                child_scaffold: sg.child_scaffold.to_string(),
            })
            .collect(),
    }
}

/// Keys that are structural noise, not selectable entities, per family (Sprint 27
/// W2): the pre-Dharma legacy-government mapping table lives in the governments
/// file; subject types' `default` template + scripted `dummy` example are not
/// real subject types (mirrors the registry exclusions).
fn family_exclude(fam: &Family) -> &'static [&'static str] {
    match fam.id {
        "governments" => &["pre_dharma_mapping"],
        "subject_types" => &["default", "dummy"],
        _ => &[],
    }
}

/// Whether a family forbids creating new entities (the game hardcodes the set),
/// so the frontend hides the create affordance. AI personalities: the vanilla
/// file states "New personalities cannot be added". The backend scaffold still
/// exists (exercised by the round-trip tests) — only the UI create path is
/// suppressed. Sprint 27 W5.
fn family_no_create(fam: &Family) -> bool {
    fam.id == "ai_personalities"
}

/// Whether a family is hidden from the Mechanics family selector (Sprint 29): the
/// three empire families (imperial reforms/incidents, decrees) are edited through
/// the dedicated View ▸ Empires overlay, but still reuse the whole config-driven
/// object editor and remain loadable by id via `get_mechanics`.
fn family_hidden(fam: &Family) -> bool {
    matches!(fam.id, "imperial_reforms" | "imperial_incidents" | "decrees")
}

/// Whether a family forward-declares every entity (`key = {}`) then defines it
/// (`key = { … }`) — so the loader must keep only the richest occurrence and
/// address edits at `key#<occurrence>`. Only subject types do this. Sprint 27 W2.
fn family_dedup(fam: &Family) -> bool {
    fam.id == "subject_types"
}

/// Ages' sub-entry containers (Sprint 27 W3). Objectives are whole-body triggers;
/// abilities carry a flat `modifier` block + effect / ai_will_do scripts.
static AGE_SUB_GROUPS: &[SubGroup] = &[
    SubGroup {
        container: "objectives",
        label: "Objectives",
        child_is_trigger: true,
        child_modifiers: &[],
        child_scripts: &[],
        child_scaffold: "{\n\t\t\talways = yes\n\t\t}",
    },
    SubGroup {
        container: "abilities",
        label: "Abilities",
        child_is_trigger: false,
        child_modifiers: &["modifier"],
        child_scripts: &[f("effect"), t("ai_will_do")],
        child_scaffold: "{\n\t\t\tmodifier = {\n\t\t\t\tnum_of_age_rewards = 1\n\t\t\t}\n\t\t\tai_will_do = {\n\t\t\t\tfactor = 10\n\t\t\t}\n\t\t}",
    },
];

/// The sub-entry containers a family exposes (only ages). Kept out of the
/// `Family` struct (keyed by id) to avoid churning the existing family literals.
fn family_sub_groups(fam: &Family) -> &'static [SubGroup] {
    match fam.id {
        "ages" => AGE_SUB_GROUPS,
        _ => &[],
    }
}

/// Per-family block inclusion filter (Sprint 27 W3). Idea groups load only the
/// generic/pickable groups — those declaring a `category` (ADM/DIP/MIL); national
/// idea sets (`TAG_ideas`, no category) are edited in the country panel instead.
fn family_filter(fam: &Family, inner: &Block) -> bool {
    match fam.id {
        "idea_groups" => inner.get_scalar("category").is_some(),
        _ => true,
    }
}

/// Count of modeled surface an object exposes — used to keep the *definition*
/// (rich) block over its empty forward declaration when de-duplicating.
fn richness(o: &MechanicObject) -> usize {
    o.self_rows.len()
        + o.scalars.iter().filter(|s| s.present).count()
        + o.script_blocks.iter().filter(|s| s.present).count()
        + o.modifier_blocks.iter().filter(|m| m.present).count()
        + o.list_fields.iter().filter(|l| l.present).count()
        + o.raw_extra.len()
}

/// Keeps only the richest object per key (stable order by first appearance).
fn dedup_richest(objects: Vec<MechanicObject>) -> Vec<MechanicObject> {
    let mut order: Vec<String> = Vec::new();
    let mut best: std::collections::HashMap<String, MechanicObject> = std::collections::HashMap::new();
    for o in objects {
        match best.get(&o.key) {
            Some(prev) if richness(prev) >= richness(&o) => {}
            Some(_) => {
                best.insert(o.key.clone(), o);
            }
            None => {
                order.push(o.key.clone());
                best.insert(o.key.clone(), o);
            }
        }
    }
    order.into_iter().filter_map(|k| best.remove(&k)).collect()
}

fn load_directory(vfs: &Vfs, loc: &LocStore, fam: &Family) -> Vec<MechanicObject> {
    let mut out = Vec::new();
    let exclude = family_exclude(fam);
    for (name, path) in vfs.list_dir(fam.dir) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let origin = origin_of(vfs, &path);
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{}/{}", fam.dir, name);
        // Per-file block occurrence per key — mirrors `mod_writer::block_span`'s
        // `key#n` addressing so an edit resolves the *nth* block of that key.
        let mut occ: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (key, b) in block.key_blocks() {
            let n = occ.entry(key).or_insert(0);
            let this_occ = *n;
            *n += 1;
            if exclude.contains(&key) || !family_filter(fam, b) {
                continue;
            }
            let edit_key =
                if this_occ > 0 { format!("{key}#{this_occ}") } else { key.to_string() };
            out.push(parse_object(&bytes, key, &edit_key, b, loc, &rel, origin, fam, None));
        }
    }
    if family_dedup(fam) {
        out = dedup_richest(out);
    }
    out
}

/// Religious schools: `<group> = { religious_schools = { <school> = { … } } }`
/// across `common/religions/*.txt`.
fn load_group_nested(vfs: &Vfs, loc: &LocStore, fam: &Family) -> Vec<MechanicObject> {
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir("common/religions") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let origin = origin_of(vfs, &path);
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("common/religions/{name}");
        for (group_key, group_block) in block.key_blocks() {
            let Some(schools) = group_block.get_block("religious_schools") else {
                continue;
            };
            for (school_key, sb) in schools.key_blocks() {
                out.push(parse_object(&bytes, school_key, school_key, sb, loc, &rel, origin, fam, Some(group_key)));
            }
        }
    }
    out
}

pub fn load(vfs: &Vfs, loc: &LocStore, family: &str) -> Result<MechanicsData, String> {
    let fam = family_for(family).ok_or_else(|| format!("Unknown mechanics family: {family}"))?;
    let objects = if fam.group_nested {
        load_group_nested(vfs, loc, fam)
    } else {
        load_directory(vfs, loc, fam)
    };
    Ok(MechanicsData { meta: family_meta(fam), objects })
}

// ---------------------------------------------------------------------------
// Event cross-reference (disasters/incidents keys referenced from events).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MechanicEventRef {
    pub file: String,
    pub origin: String,
    /// Number of times the key appears as a bare token in the file.
    pub count: usize,
}

/// Scans `events/*.txt` for the mechanic `key` as a whole word (e.g.
/// `has_disaster = <key>` / `start_disaster = <key>`), returning the files that
/// reference it. Honest, byte-level; no semantic filtering.
pub fn event_references(vfs: &Vfs, key: &str) -> Vec<MechanicEventRef> {
    let mut out = Vec::new();
    let mod_dir = vfs.mod_dir();
    for (name, path) in vfs.list_dir("events") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let count = count_word(&text, key);
        if count > 0 {
            let origin = if mod_dir.is_some_and(|md| path.starts_with(md)) { "mod" } else { "base" };
            out.push(MechanicEventRef {
                file: format!("events/{name}"),
                origin: origin.to_string(),
                count,
            });
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

/// Counts whole-word occurrences of `word` (bounded by non-identifier chars).
fn count_word(text: &str, word: &str) -> usize {
    if word.is_empty() {
        return 0;
    }
    let bytes = text.as_bytes();
    let wb = word.as_bytes();
    let is_ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut count = 0;
    let mut i = 0;
    while let Some(pos) = text[i..].find(word) {
        let start = i + pos;
        let end = start + wb.len();
        let before_ok = start == 0 || !is_ident(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_ident(bytes[end]);
        if before_ok && after_ok {
            count += 1;
        }
        i = start + 1;
    }
    count
}

// ---------------------------------------------------------------------------
// Scaffolds (minimal game-valid blocks; unit-tested to parse back).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct LocEntry {
    pub key: String,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scaffold {
    pub key: String,
    pub file: String,
    pub text: String,
    pub loc_entries: Vec<LocEntry>,
    /// For group-nested families: the religion group the school scaffolds into,
    /// so the frontend targets `InsertStatement` at `[group, "religious_schools"]`.
    pub group: Option<String>,
    pub group_nested: bool,
    /// Named-sprite emission (buildings/institutions, Sprint 27): a self-contained
    /// `spriteTypes = { spriteType = { … } }` block the frontend appends to
    /// `gfx_file` (createFile if absent, appendText if the file is already
    /// pending) so the created entity's icon resolves in game. `None` when the
    /// family resolves its icon some other way (reforms: named ref) or not at all.
    pub gfx_file: Option<String>,
    pub gfx_text: Option<String>,
}

/// The loc desc key for a family+entity (Sprint 27 desc pattern).
fn desc_key_of(fam: &Family, key: &str) -> String {
    if fam.desc_suffix {
        format!("{key}_desc")
    } else {
        format!("desc_{key}")
    }
}

fn scaffold_common(fam: &Family, key: &str, text: String, group: Option<String>) -> Scaffold {
    let pretty = loc::prettify(key);
    let (gfx_file, gfx_text) = match &fam.icon_gfx {
        Some(g) => (
            Some(g.gfx_file.to_string()),
            Some(gfx_sprite_block(g, key)),
        ),
        None => (None, None),
    };
    Scaffold {
        key: key.to_string(),
        file: fam.project_file.to_string(),
        text,
        loc_entries: vec![
            LocEntry { key: key.to_string(), value: pretty.clone() },
            LocEntry { key: desc_key_of(fam, key), value: format!("{pretty}.") },
        ],
        group,
        group_nested: fam.group_nested,
        gfx_file,
        gfx_text,
    }
}

/// A self-contained `spriteTypes` block naming `<prefix><key>` at the family's
/// base texture. EU4 `.gfx` files legally concatenate multiple `spriteTypes`
/// blocks, so appending one per created entity keeps the file valid.
fn gfx_sprite_block(g: &IconGfx, key: &str) -> String {
    format!(
        "spriteTypes = {{\n\
\tspriteType = {{\n\
\t\tname = \"{prefix}{key}\"\n\
\t\ttexturefile = \"{texture}\"\n\
\t\tloadType = \"INGAME\"\n\
\t}}\n\
}}",
        prefix = g.sprite_prefix,
        texture = g.texture,
    )
}

fn scaffold_disaster(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tpotential = {{\n\t\talways = no\n\t}}\n\
\tcan_start = {{\n\t\talways = no\n\t}}\n\
\tcan_stop = {{\n\t\talways = yes\n\t}}\n\
\tprogress = {{\n\t\tmodifier = {{\n\t\t\tfactor = 1\n\t\t\talways = yes\n\t\t}}\n\t}}\n\
\tcan_end = {{\n\t\talways = yes\n\t}}\n\
\tmodifier = {{\n\t\tglobal_unrest = 1\n\t}}\n\
\ton_start = {key}.1\n\
\ton_end = {key}.2\n\
\ton_monthly = {{\n\t\tevents = {{\n\t\t}}\n\t}}\n\
}}"
    )
}

fn scaffold_parliament_issue(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcategory = 3\n\
\tallow = {{\n\t\talways = yes\n\t}}\n\
\teffect = {{\n\t\tadd_stability_or_adm_power = yes\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_parliament_bribe(key: &str) -> String {
    format!(
        "{key} = {{\n\
\ttrigger = {{\n\t\thas_reached_seat_threshold = no\n\t}}\n\
\teffect = {{\n\t\tadd_prestige = 5\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_faction(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tmonarch_power = ADM\n\
\talways = yes\n\
\tmodifier = {{\n\t\tglobal_tax_modifier = 0.1\n\t}}\n\
}}"
    )
}

fn scaffold_deity(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tglobal_tax_modifier = 0.1\n\
\tpotential = {{\n\t}}\n\
\ttrigger = {{\n\t}}\n\
\tsprite = 1\n\
\teffect = {{\n\t}}\n\
\tremoved_effect = {{\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_church_aspect(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcost = 100\n\
\teffect = {{\n\t\tadd_prestige = 5\n\t}}\n\
\tmodifier = {{\n\t\tdevelopment_cost = -0.05\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_cult(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tland_attrition = -0.15\n\
\tallow = {{\n\t\treligion = shamanism\n\t}}\n\
\tsprite = 1\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_holy_order(key: &str) -> String {
    format!(
        "{key} = {{\n\
\ticon = GFX_holy_order_benedictines\n\
\ttrigger = {{\n\t\treligion_group = christian\n\t}}\n\
\tcolor = {{ 140 150 120 }}\n\
\tcost = 50\n\
\tcost_type = adm_power\n\
\tper_province_effect = {{\n\t\tadd_base_tax = 1\n\t}}\n\
\tper_province_abandon_effect = {{\n\t\tadd_base_tax = -1\n\t}}\n\
\tmodifier = {{\n\t\tlocal_unrest = -1\n\t}}\n\
\tai_priority = {{\n\t\tfactor = 1\n\t}}\n\
\tlocalization = holy_order\n\
}}"
    )
}

fn scaffold_fervor(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcost = 8\n\
\tpotential = {{\n\t}}\n\
\ttrigger = {{\n\t}}\n\
\teffect = {{\n\t\tglobal_trade_power = 0.1\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_isolationism(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tisolation_value = 0\n\
\tmodifier = {{\n\t\ttechnology_cost = -0.05\n\t}}\n\
}}"
    )
}

fn scaffold_golden_bull(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tmodifier = {{\n\t\tae_impact = -0.1\n\t}}\n\
\tpotential = {{\n\t}}\n\
\ttrigger = {{\n\t\tis_papal_controller = yes\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 0\n\t}}\n\
}}"
    )
}

fn scaffold_religious_reform(key: &str) -> String {
    format!(
        "{key} = {{\n\
\ttrigger = {{\n\t\talways = no\n\t}}\n\
\tcan_buy_idea = {{\n\t\talways = yes\n\t}}\n\
\t{key}_1 = {{\n\t\twar_exhaustion = -0.05\n\t}}\n\
\t{key}_2 = {{\n\t\tdiplomatic_upkeep = 1\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_incident(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tframe = 1\n\
\tvariable_initial = 0\n\
\tpotential = {{\n\t\treligion = shinto\n\t}}\n\
\ttrigger = {{\n\t\talways = no\n\t}}\n\
\tmean_time_to_happen = {{\n\t\tmonths = 200\n\t}}\n\
\timmediate_effect = {{\n\t\tset_country_flag = active_incident_flag\n\t}}\n\
}}"
    )
}

fn scaffold_school(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tpotential_invite_scholar = {{\n\t\talways = yes\n\t}}\n\
\tcan_invite_scholar = {{\n\t\tadm_power_cost = 50\n\t}}\n\
\ton_invite_scholar = {{\n\t\tadm_power_cost = 50\n\t}}\n\
\tpicture = \"GFX_icon_muslim_school_hanafi\"\n\
\tadm_tech_cost_modifier = -0.05\n\
}}"
    )
}

fn scaffold_government_reform(key: &str) -> String {
    format!(
        "{key} = {{\n\
\ticon = \"crown\"\n\
\tallow_normal_conversion = yes\n\
\tlegacy_government = no\n\
\tvalid_for_new_country = yes\n\
\tvalid_for_nation_designer = yes\n\
\tnation_designer_cost = 0\n\
\tmonarchy = yes\n\
\tpotential = {{\n\t\talways = yes\n\t}}\n\
\ttrigger = {{\n\t\talways = yes\n\t}}\n\
\tmodifiers = {{\n\t\tglobal_unrest = -1\n\t}}\n\
\tai = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_building(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcost = 100\n\
\ttime = 12\n\
\tmodifier = {{\n\t\tlocal_defensiveness = 0.1\n\t}}\n\
\ton_built = {{\n\t}}\n\
\ton_destroyed = {{\n\t}}\n\
\ton_obsolete = {{\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_institution(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tbonus = {{\n\t\tdevelopment_cost = -0.05\n\t}}\n\
\ttrade_company_efficiency = 0.2\n\
\thistorical_start_date = 1500.1.1\n\
\thistorical_start_province = 1\n\
\thistory = {{\n\t\tis_year = 1500\n\t}}\n\
\tcan_start = {{\n\t\tis_year = 1500\n\t}}\n\
\tstart_chance = 5\n\
\tcan_embrace = {{\n\t\talways = yes\n\t}}\n\
\tembracement_speed = {{\n\t\tmodifier = {{\n\t\t\tfactor = 1\n\t\t\talways = yes\n\t\t}}\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

// ---- Sprint 27 Wave 2 scaffolds ------------------------------------------

fn scaffold_government(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcolor = {{ 100 100 100 }}\n\
\tbasic_reform = monarchy_mechanic\n\
\tlegacy_government = {{\n\t\tdespotic_monarchy\n\t}}\n\
\treform_levels = {{\n\t\tbasic_reforms = {{\n\t\t\treforms = {{\n\t\t\t\tfeudalism_reform\n\t\t\t}}\n\t\t}}\n\t}}\n\
}}"
    )
}

fn scaffold_subject_type(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcopy_from = vassal\n\
\tsprite = GFX_icon_vassal\n\
\tis_potential_overlord = {{\n\t\talways = no\n\t}}\n\
\trelative_power_class = 1\n\
\tjoins_overlords_wars = yes\n\
\tcan_be_annexed = yes\n\
\tmodifier_subject = {{\n\t\tland_morale = -0.1\n\t}}\n\
}}"
    )
}

fn scaffold_subject_type_upgrade(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcan_upgrade_trigger = {{\n\t\tis_subject_of_type = vassal\n\t}}\n\
\tcost = 100\n\
\teffect = {{\n\t\tadd_adm_power = -25\n\t}}\n\
\tremoved_effect = {{\n\t}}\n\
\tmodifier_overlord = {{\n\t\tland_forcelimit = 5\n\t}}\n\
\tmodifier_subject = {{\n\t\tliberty_desire = 10\n\t}}\n\
}}"
    )
}

fn scaffold_cb_type(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tvalid_for_subject = no\n\
\tis_triggered_only = yes\n\
\tmonths = 120\n\
\tprerequisites = {{\n\t\tFROM = {{ is_subject = no }}\n\t}}\n\
\tprerequisites_self = {{\n\t\tis_subject = no\n\t}}\n\
\twar_goal = take_claim\n\
}}"
    )
}

fn scaffold_wargoal_type(key: &str) -> String {
    format!(
        "{key} = {{\n\
\ttype = take_province\n\
\twar_name = ACQUIRE_WARNAME\n\
\tallowed_provinces = {{\n\t\tis_claim = yes\n\t}}\n\
\tattacker = {{\n\t\tbadboy_factor = 1\n\t\tprestige_factor = 1\n\t\tpeace_cost_factor = 1\n\t\tpeace_options = {{\n\t\t\tpo_demand_provinces\n\t\t}}\n\t}}\n\
\tdefender = {{\n\t\tbadboy_factor = 1\n\t\tprestige_factor = 1\n\t\tpeace_cost_factor = 1\n\t\tpeace_options = {{\n\t\t\tpo_demand_provinces\n\t\t}}\n\t}}\n\
}}"
    )
}

fn scaffold_peace_treaty(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcategory = 6\n\
\tpower_cost_base = 1.0\n\
\tprestige_base = 0.1\n\
\tae_base = 1.0\n\
\twarscore_cost = {{\n\t\tno_provinces = 20.0\n\t}}\n\
\twarscore_cap = -1\n\
\tis_visible = {{\n\t\talways = yes\n\t}}\n\
\tis_allowed = {{\n\t\talways = yes\n\t}}\n\
\teffect = {{\n\t}}\n\
\tai_weight = {{\n\t\texport_to_variable = {{\n\t\t\tvariable_name = ai_value\n\t\t\tvalue = 0\n\t\t}}\n\t}}\n\
}}"
    )
}

fn scaffold_policy(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tmonarch_power = ADM\n\
\tpotential = {{\n\t\thas_idea_group = aristocracy_ideas\n\t}}\n\
\tallow = {{\n\t\tfull_idea_group = aristocracy_ideas\n\t}}\n\
\tglobal_tax_modifier = 0.1\n\
\teffect = {{\n\t}}\n\
\tremoved_effect = {{\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_powerprojection(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tpower = 10\n\
\tmax = 25\n\
}}"
    )
}

// ---- Sprint 27 Wave 3 scaffolds ------------------------------------------

fn scaffold_idea_group(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcategory = ADM\n\
\tstart = {{\n\t\tglobal_tax_modifier = 0.1\n\t}}\n\
\tbonus = {{\n\t\tland_morale = 0.1\n\t}}\n\
\ttrigger = {{\n\t\talways = yes\n\t}}\n\
\tfree = yes\n\
\t{key}_idea_1 = {{\n\t\tglobal_unrest = -1\n\t}}\n\
\t{key}_idea_2 = {{\n\t\tdevelopment_cost = -0.05\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_advisortype(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tmonarch_power = ADM\n\
\tprestige = 1\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_ruler_personality(key: &str) -> String {
    format!(
        "{key} = {{\n\
\truler_allow = {{\n\t\tchance = {{\n\t\t\tmodifier = {{\n\t\t\t\tfactor = 1\n\t\t\t\tADM = 4\n\t\t\t}}\n\t\t}}\n\t}}\n\
\their_allow = {{\n\t\tchance = {{\n\t\t\tmodifier = {{\n\t\t\t\tfactor = 1\n\t\t\t\their_ADM = 4\n\t\t\t}}\n\t\t}}\n\t}}\n\
\tglobal_unrest = -1\n\
\tnation_designer_cost = 2\n\
}}"
    )
}

fn scaffold_leader_personality(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tallow = {{\n\t\tis_admiral = no\n\t}}\n\
\tland_morale = 0.05\n\
}}"
    )
}

fn scaffold_flat_modifier(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tglobal_tax_modifier = 0.1\n\
}}"
    )
}

fn scaffold_opinion_modifier(key: &str) -> String {
    format!(
        "{key} = {{\n\
\topinion = 25\n\
\tyearly_decay = 1\n\
}}"
    )
}

fn scaffold_triggered_modifier(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tpotential = {{\n\t\talways = no\n\t}}\n\
\ttrigger = {{\n\t\talways = no\n\t}}\n\
\ttrade_efficiency = 0.05\n\
}}"
    )
}

fn scaffold_age(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tstart = 1500\n\
\tcan_start = {{\n\t\talways = no\n\t}}\n\
\tpapacy = 1.0\n\
\treligious_conflicts = yes\n\
\tobjectives = {{\n\t\t{key}_obj_1 = {{\n\t\t\talways = yes\n\t\t}}\n\t}}\n\
\tabilities = {{\n\t\t{key}_ab_1 = {{\n\t\t\tmodifier = {{\n\t\t\t\tnum_of_age_rewards = 1\n\t\t\t}}\n\t\t\tai_will_do = {{\n\t\t\t\tfactor = 10\n\t\t\t}}\n\t\t}}\n\t}}\n\
}}"
    )
}

fn scaffold_hegemon(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tallow = {{\n\t\tis_great_power = yes\n\t}}\n\
\tbase = {{\n\t\twar_exhaustion = -0.1\n\t}}\n\
\tscale = {{\n\t\tglobal_trade_goods_size_modifier = 0.25\n\t}}\n\
\tmax = {{\n\t\tgoverning_capacity_modifier = 0.2\n\t}}\n\
}}"
    )
}

fn scaffold_government_rank(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tgoverning_capacity = 200\n\
\tglobal_autonomy = -0.025\n\
}}"
    )
}

fn scaffold_state_edict(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tpotential = {{\n\t\talways = yes\n\t}}\n\
\tallow = {{\n\t\talways = yes\n\t}}\n\
\tmodifier = {{\n\t\tlocal_autonomy = -0.03\n\t}}\n\
\tcolor = {{ 160 120 90 }}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_natives(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tgraphical_culture = northamericagfx\n\
\tcolor = {{ 0 255 0 }}\n\
\ticon = 1\n\
\tunit = native_indian_archer\n\
\tprovinces = {{\n\t}}\n\
}}"
    )
}

// ---- Sprint 27 Wave 4 scaffolds ------------------------------------------

fn scaffold_trading_policy(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tpotential = {{\n\t\talways = yes\n\t}}\n\
\tcan_select = {{\n\t\talways = yes\n\t}}\n\
\ttrade_power = {{\n\t\tduration = -1\n\t\tpower_modifier = 0.05\n\t\tkey = {key}\n\t}}\n\
\tcenter_of_reformation = no\n\
\tbutton_gfx = GFX_Trading_Policy_Max_Profit\n\
\tcooldown = no\n\
}}"
    )
}

fn scaffold_tradecompany_investment(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcategory = company_garrison\n\
\tsprite = \"GFX_investment_local_quarter\"\n\
\tcost = 200.0\n\
\tcompany_province_area_modifier = {{\n\t\tlocal_defensiveness = 0.15\n\t}}\n\
\tai_global_worth = {{\n\t\tfactor = 0\n\t}}\n\
\tai_area_worth = {{\n\t\tfactor = 1\n\t}}\n\
\tai_region_worth = {{\n\t\tfactor = 0\n\t}}\n\
}}"
    )
}

fn scaffold_center_of_trade(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tlevel = 1\n\
\ttype = coastal\n\
\tprovince_modifiers = {{\n\t\tprovince_trade_power_value = 5\n\t}}\n\
}}"
    )
}

fn scaffold_naval_doctrine(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcan_select = {{\n\t\tis_primitive = no\n\t}}\n\
\tcost = 0.1\n\
\tcountry_modifier = {{\n\t\tnaval_maintenance_modifier = -0.1\n\t}}\n\
\teffect = {{\n\t}}\n\
\tremoved_effect = {{\n\t}}\n\
\tbutton_gfx = 1\n\
}}"
    )
}

fn scaffold_professionalism(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tarmy_professionalism = 0.2\n\
\tmarker_sprite = GFX_pa_rank_1\n\
\tunit_sprite_start = \"GFX_ap2_\"\n\
\ttrigger = {{\n\t\talways = yes\n\t}}\n\
\tmay_build_supply_depot = yes\n\
}}"
    )
}

fn scaffold_flagship_modification(key: &str) -> String {
    format!(
        "{key} = {{\n\
\ttrigger = {{\n\t\talways = yes\n\t}}\n\
\tmodifier = {{\n\t\tflagship_durability = 1\n\t}}\n\
\tai_trade_score = {{\n\t\tfactor = 1\n\t}}\n\
\tai_war_score = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

// ---- Sprint 27 Wave 5 scaffolds ------------------------------------------

fn scaffold_custom_ideas(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcategory = ADM\n\
\tcustom_idea_{key}_1 = {{\n\t\tglobal_tax_modifier = 0.05\n\t\tlevel_cost_2 = 3\n\t\tlevel_cost_3 = 9\n\t\tlevel_cost_4 = 18\n\t\tdefault = 2\n\t\tchance = {{\n\t\t\tfactor = 1\n\t\t}}\n\t}}\n\
}}"
    )
}

fn scaffold_ai_personality(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tchance = {{\n\t\tfactor = 0\n\t}}\n\
\ticon = 1\n\
}}"
    )
}

/// Minimal imperial reform (Sprint 29). The chain-aware create flow in the
/// Empires overlay uses `empires::scaffold_imperial_reform` instead (it sets
/// `empire` + `required_reform`); this bare form only backs the round-trip tests.
fn scaffold_imperial_reform_min(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tempire = hre\n\
\tpotential = {{\n\t\talways = yes\n\t}}\n\
\temperor = {{\n\t\timperial_authority_value = 1\n\t}}\n\
}}"
    )
}

fn scaffold_imperial_incident(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tevent = {key}.1\n\
\tdefault_option = 0\n\
\tcan_stop = {{\n\t\talways = no\n\t}}\n\
\t0 = {{\n\t\tfactor = 1\n\t}}\n\
\t1 = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    )
}

fn scaffold_decree(key: &str) -> String {
    format!(
        "{key} = {{\n\
\tcost = 20\n\
\tduration = 3650\n\
\tpotential = {{\n\t\talways = yes\n\t}}\n\
\ttrigger = {{\n\t}}\n\
\tmodifier = {{\n\t\tdevelopment_cost = -0.1\n\t}}\n\
\teffect = {{\n\t}}\n\
\tremoved_effect = {{\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 10\n\t}}\n\
}}"
    )
}

fn scaffold_insult(key: &str) -> String {
    format!(
        "{key} = {{\n\
\ttrigger = {{\n\t\talways = yes\n\t}}\n\
}}"
    )
}

pub fn scaffold(family: &str, key: &str, group: Option<&str>) -> Result<Scaffold, String> {
    let fam = family_for(family).ok_or_else(|| format!("Unknown mechanics family: {family}"))?;
    let text = (fam.scaffold)(key);
    Ok(scaffold_common(fam, key, text, group.map(str::to_string)))
}

// ---------------------------------------------------------------------------
// Commands.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_mechanic_families() -> Vec<FamilyMeta> {
    FAMILIES.iter().filter(|f| !family_hidden(f)).map(family_meta).collect()
}

/// The meta for a single family by id (Sprint 29 — the Empires overlay renders
/// the hidden empire families, which `get_mechanic_families` omits).
#[tauri::command]
pub fn get_mechanic_family(family: String) -> Result<FamilyMeta, String> {
    family_for(&family)
        .map(family_meta)
        .ok_or_else(|| format!("Unknown mechanics family: {family}"))
}

#[tauri::command]
pub fn get_mechanics(
    install_path: String,
    mod_path: Option<String>,
    family: String,
) -> Result<MechanicsData, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    load(&vfs, &loc, &family)
}

#[tauri::command]
pub fn scaffold_mechanic(
    family: String,
    key: String,
    group: Option<String>,
) -> Result<Scaffold, String> {
    scaffold(&family, &key, group.as_deref())
}

#[tauri::command]
pub fn find_mechanic_event_refs(
    install_path: String,
    mod_path: Option<String>,
    key: String,
) -> Result<Vec<MechanicEventRef>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(event_references(&vfs, &key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply, Edit};
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").is_file()
    }

    fn synthetic(name: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_mechanics_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (rel, contents) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    // ---- scaffolds all parse back with their entity key ----------------------

    #[test]
    fn every_family_scaffold_parses_back() {
        for fam in FAMILIES {
            let key = format!("{}_test_key", fam.id);
            let sc = scaffold(fam.id, &key, Some("muslim")).unwrap();
            let b = paradox::parse(&sc.text);
            assert!(
                b.get_block(&key).is_some(),
                "{} scaffold must parse back to a {key} block:\n{}",
                fam.id,
                sc.text
            );
            // loc entries include the name + desc keys (per the family's pattern).
            assert!(sc.loc_entries.iter().any(|e| e.key == key));
            assert!(sc.loc_entries.iter().any(|e| e.key == desc_key_of(fam, &key)));
        }
    }

    #[test]
    fn every_family_scaffold_create_then_delete_is_identity() {
        for fam in FAMILIES {
            // Group-nested scaffolds insert into an existing block, not append —
            // covered separately.
            if fam.group_nested {
                continue;
            }
            let key = format!("{}_rt_key", fam.id);
            let base = "existing = {\n\tfoo = bar\n}\n";
            let sc = scaffold(fam.id, &key, None).unwrap();
            let appended = apply(base.as_bytes(), &Edit::Append { text: sc.text.clone() }).unwrap();
            assert!(String::from_utf8_lossy(&appended).contains(&format!("{key} = {{")));
            let deleted = apply(
                &appended,
                &Edit::RemoveStatement { block_path: vec![], key: key.clone(), value: None },
            )
            .unwrap();
            assert_eq!(deleted, base.as_bytes(), "{}: create then delete restores source", fam.id);
        }
    }

    // ---- disasters -----------------------------------------------------------

    const DISASTER_SRC: &str = "\
court_and_country = {\n\
\tpotential = {\n\t\tnum_of_cities = 8\n\t}\n\
\tcan_start = {\n\t\tabsolutism = 50\n\t}\n\
\tcan_stop = {\n\t\tstability = 3\n\t}\n\
\tcan_end = {\n\t\tstability = 0\n\t}\n\
\tprogress = {\n\t\tmodifier = { factor = 1 absolutism = 50 }\n\t}\n\
\tmodifier = {\n\t\tglobal_tax_modifier = -0.2\n\t\tmax_absolutism = -20\n\t}\n\
\ton_start = court_and_country_events.1\n\
\ton_end = court_and_country_events.100\n\
\ton_monthly = {\n\t\tevents = {}\n\t}\n\
}\n";

    #[test]
    fn parses_disaster_typed_scripts_modifier_and_event_refs() {
        let (_root, vfs) = synthetic("disaster", &[("common/disasters/00_test.txt", DISASTER_SRC)]);
        let loc = LocStore::from_pairs(&[("court_and_country", "Court and Country")]);
        let data = load(&vfs, &loc, "disasters").unwrap();
        assert_eq!(data.objects.len(), 1);
        let d = &data.objects[0];
        assert_eq!(d.name, "Court and Country");
        // triggers/weights present
        let sb = |k: &str| d.script_blocks.iter().find(|s| s.name == k).unwrap();
        assert!(sb("potential").present && sb("potential").registry == "triggers");
        assert!(sb("progress").present);
        // modifier block typed + flat
        let m = d.modifier_blocks.iter().find(|m| m.name == "modifier").unwrap();
        assert!(m.present && m.flat);
        assert!(m.rows.iter().any(|r| r.key == "max_absolutism" && r.value == "-20"));
        // event references
        let er = |k: &str| d.event_refs.iter().find(|e| e.key == k).unwrap();
        assert_eq!(er("on_start").id, "court_and_country_events.1");
        assert_eq!(er("on_end").id, "court_and_country_events.100");
        // on_monthly preserved raw
        assert!(d.raw_extra.contains(&"on_monthly".to_string()));
        assert!(d.raw.starts_with('{') && d.raw.ends_with('}'));
    }

    #[test]
    fn disaster_scalar_and_modifier_edits_are_byte_surgical() {
        let out = apply(
            DISASTER_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["court_and_country".into(), "on_start".into()],
                value: "my_events.5".into(),
                quoted: false,
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock {
                path: vec!["court_and_country".into(), "modifier".into()],
                value: "global_tax_modifier = -0.3".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("on_start = my_events.5"));
        assert!(text.contains("global_tax_modifier = -0.3"));
        // untouched siblings round-trip
        assert!(text.contains("on_end = court_and_country_events.100"));
        assert!(text.contains("num_of_cities = 8"));
    }

    // ---- self_modifier (deities / cults) -------------------------------------

    const DEITY_SRC: &str = "\
odin = {\n\
\tlegitimacy = 0.1\n\
\thorde_unity = 0.1\n\
\tcore_creation = -0.10\n\
\tpotential = { religion = norse_pagan_reformed }\n\
\ttrigger = {}\n\
\tsprite = 7\n\
\teffect = {}\n\
\tremoved_effect = {}\n\
\tai_will_do = { factor = 1 }\n\
}\n";

    #[test]
    fn parses_deity_self_modifier_rows() {
        let (_root, vfs) = synthetic("deity", &[("common/personal_deities/00_test.txt", DEITY_SRC)]);
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc, "personal_deities").unwrap();
        let d = data.objects.iter().find(|o| o.key == "odin").unwrap();
        assert!(d.self_modifier);
        // flat modifiers surfaced as self_rows, sprite is a structural scalar.
        let keys: Vec<&str> = d.self_rows.iter().map(|r| r.key.as_str()).collect();
        assert!(keys.contains(&"legitimacy"));
        assert!(keys.contains(&"horde_unity"));
        assert!(keys.contains(&"core_creation"));
        assert!(!keys.contains(&"sprite"), "sprite is a structural scalar, not a modifier row");
        assert_eq!(d.scalars.iter().find(|s| s.key == "sprite").unwrap().value, "7");
        // structural blocks not treated as modifiers; nothing spurious in raw.
        assert!(d.raw_extra.is_empty(), "deity has no unmodeled keys, got {:?}", d.raw_extra);
        // per-row scalar edit is byte-surgical.
        let out = apply(
            DEITY_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["odin".into(), "legitimacy".into()], value: "0.2".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("legitimacy = 0.2"));
        assert!(text.contains("horde_unity = 0.1"));
    }

    // ---- ordered children (religious reforms) --------------------------------

    const REFORM_SRC: &str = "\
nahuatl_reforms = {\n\
\ttrigger = { religion = nahuatl }\n\
\tcan_buy_idea = { stability = 1 }\n\
\tnahuatl_reform_1 = { war_exhaustion = -0.05 }\n\
\tnahuatl_reform_2 = { diplomatic_upkeep = 1 }\n\
\tnahuatl_reform_3 = { discipline = 0.05 }\n\
\tai_will_do = { factor = 1 }\n\
}\n";

    #[test]
    fn parses_religious_reform_ordered_children() {
        let (_root, vfs) =
            synthetic("reform", &[("common/religious_reforms/00_test.txt", REFORM_SRC)]);
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc, "religious_reforms").unwrap();
        let r = data.objects.iter().find(|o| o.key == "nahuatl_reforms").unwrap();
        assert!(r.ordered);
        let names: Vec<&str> = r.ordered_children.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(names, vec!["nahuatl_reform_1", "nahuatl_reform_2", "nahuatl_reform_3"]);
        assert!(r.ordered_children[0].flat);
        assert_eq!(r.ordered_children[0].rows[0].key, "war_exhaustion");
        // trigger + can_buy_idea are structural, not children.
        assert!(r.script_blocks.iter().find(|s| s.name == "can_buy_idea").unwrap().present);
        // a reform step modifier edit is byte-surgical.
        let out = apply(
            REFORM_SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["nahuatl_reforms".into(), "nahuatl_reform_2".into()],
                value: "diplomatic_upkeep = 2".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("diplomatic_upkeep = 2"));
        assert!(text.contains("war_exhaustion = -0.05"));
    }

    // ---- group-nested (religious schools) ------------------------------------

    const RELIGION_SRC: &str = "\
muslim = {\n\
\tdefender_of_faith = yes\n\
\treligious_schools = {\n\
\t\thanafi_school = {\n\
\t\t\tpotential_invite_scholar = { always = yes }\n\
\t\t\tcan_invite_scholar = { adm_power_cost = 50 }\n\
\t\t\ton_invite_scholar = { adm_power_cost = 50 }\n\
\t\t\tpicture = \"GFX_icon_muslim_school_hanafi\"\n\
\t\t\tadm_tech_cost_modifier = -0.05\n\
\t\t}\n\
\t\tmaliki_school = {\n\
\t\t\tpicture = \"GFX_icon_muslim_school_maliki\"\n\
\t\t\tland_morale = 0.05\n\
\t\t}\n\
\t}\n\
}\n\
sunni = {\n\
\ticon = 1\n\
}\n";

    #[test]
    fn parses_group_nested_religious_schools() {
        let (_root, vfs) = synthetic("schools", &[("common/religions/00_religion.txt", RELIGION_SRC)]);
        let loc = LocStore::from_pairs(&[("hanafi_school", "Hanafi")]);
        let data = load(&vfs, &loc, "religious_schools").unwrap();
        assert_eq!(data.objects.len(), 2);
        let h = data.objects.iter().find(|o| o.key == "hanafi_school").unwrap();
        assert_eq!(h.group.as_deref(), Some("muslim"));
        assert_eq!(h.name, "Hanafi");
        // The paradox parser strips surrounding quotes from scalar values.
        assert_eq!(h.scalars.iter().find(|s| s.key == "picture").unwrap().value, "GFX_icon_muslim_school_hanafi");
        assert!(h.self_modifier);
        assert!(h.self_rows.iter().any(|r| r.key == "adm_tech_cost_modifier"));
        assert!(h.script_blocks.iter().find(|s| s.name == "can_invite_scholar").unwrap().present);
        // raw span is the nested block.
        assert!(h.raw.contains("adm_tech_cost_modifier"));
        // A nested scalar edit resolves through the group path.
        let out = apply(
            RELIGION_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["muslim".into(), "religious_schools".into(), "hanafi_school".into(), "adm_tech_cost_modifier".into()],
                value: "-0.1".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("adm_tech_cost_modifier = -0.1"));
        assert!(text.contains("GFX_icon_muslim_school_maliki"), "sibling school untouched");
    }

    #[test]
    fn group_nested_scaffold_inserts_into_group() {
        let sc = scaffold("religious_schools", "my_school", Some("muslim")).unwrap();
        assert!(sc.group_nested);
        assert_eq!(sc.group.as_deref(), Some("muslim"));
        // Inserting the scaffold into the religious_schools block round-trips.
        let out = apply(
            RELIGION_SRC.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec!["muslim".into(), "religious_schools".into()],
                statement: sc.text.clone(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("my_school = {"));
        assert!(text.contains("hanafi_school = {"), "existing school preserved");
    }

    // ---- event references ----------------------------------------------------

    #[test]
    fn event_references_whole_word_scan() {
        let (_root, vfs) = synthetic(
            "eventrefs",
            &[(
                "events/court_events.txt",
                "namespace = court\ncountry_event = { id = court.1\n\ttrigger = { has_disaster = court_and_country }\n}\n# not_court_and_country_extra should not match\n",
            )],
        );
        let refs = event_references(&vfs, "court_and_country");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].file, "events/court_events.txt");
        assert_eq!(refs[0].count, 1, "the substring in a longer identifier must not match");
    }

    // ---- generic family metadata ---------------------------------------------

    #[test]
    fn family_ids_include_sprint26_and_wave1() {
        let ids = family_ids();
        assert_eq!(ids.len(), 50);
        for expected in [
            "disasters",
            "parliament_issues",
            "parliament_bribes",
            "factions",
            "personal_deities",
            "church_aspects",
            "fetishist_cults",
            "holy_orders",
            "fervor",
            "isolationism",
            "golden_bulls",
            "religious_reforms",
            "incidents",
            "religious_schools",
            // Sprint 27 Wave 1
            "government_reforms",
            "buildings",
            "institutions",
            // Sprint 27 Wave 2
            "governments",
            "subject_types",
            "subject_type_upgrades",
            "cb_types",
            "wargoal_types",
            "peace_treaties",
            "policies",
            "powerprojection",
            // Sprint 27 Wave 3
            "idea_groups",
            "advisortypes",
            "ruler_personalities",
            "leader_personalities",
            "event_modifiers",
            "opinion_modifiers",
            "static_modifiers",
            "triggered_modifiers",
            "ages",
            "hegemons",
            "government_ranks",
            "state_edicts",
            "natives",
            // Sprint 27 Wave 4
            "trading_policies",
            "tradecompany_investments",
            "centers_of_trade",
            "naval_doctrines",
            "professionalism",
            "flagship_modifications",
            // Sprint 27 Wave 5
            "custom_ideas",
            "ai_personalities",
            "insults",
            // Sprint 29 (Empires — hidden from the Mechanics selector)
            "imperial_reforms",
            "imperial_incidents",
            "decrees",
        ] {
            assert!(ids.contains(&expected), "missing family {expected}");
        }
    }

    // ---- Sprint 27 Wave 5: create-affordance gate ----------------------------

    #[test]
    fn ai_personalities_forbids_create_others_allow() {
        let metas = get_mechanic_families();
        let ai = metas.iter().find(|m| m.id == "ai_personalities").unwrap();
        assert!(!ai.allow_create, "AI personalities are hardcoded — no create affordance");
        // A representative editable family keeps create.
        let ins = metas.iter().find(|m| m.id == "insults").unwrap();
        assert!(ins.allow_create);
    }

    // ---- Sprint 29: empire families hidden from the Mechanics selector -------

    #[test]
    fn empire_families_hidden_from_selector_but_loadable() {
        let selector: Vec<String> = get_mechanic_families().into_iter().map(|m| m.id).collect();
        for id in ["imperial_reforms", "imperial_incidents", "decrees"] {
            assert!(!selector.contains(&id.to_string()), "{id} must be hidden from the selector");
            assert!(family_ids().contains(&id), "{id} must still exist");
            // Loadable by id (drives the Empires overlay).
            assert!(get_mechanic_family(id.to_string()).is_ok());
        }
    }

    // ---- vanilla full parse per family ---------------------------------------

    #[test]
    fn vanilla_loads_every_directory_family() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        for fam in FAMILIES {
            let data = load(&vfs, &loc, fam.id).unwrap();
            assert!(!data.objects.is_empty(), "{}: expected objects, got 0", fam.id);
            // Every object round-trips its raw span without panicking.
            for o in &data.objects {
                assert!(o.raw.starts_with('{') && o.raw.ends_with('}'), "{}: {} raw span", fam.id, o.key);
            }
        }
    }

    #[test]
    fn vanilla_disaster_court_and_country_typed() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let data = load(&vfs, &loc, "disasters").unwrap();
        let d = data.objects.iter().find(|o| o.key == "court_and_country").expect("court_and_country");
        assert!(d.script_blocks.iter().find(|s| s.name == "can_start").unwrap().present);
        assert!(d.event_refs.iter().any(|e| e.key == "on_start"));
        // Referenced from the court_and_country events file.
        let refs = event_references(&vfs, "court_and_country");
        assert!(refs.iter().any(|r| r.file.contains("court_and_country")), "disaster referenced by events, got {refs:?}");
    }

    #[test]
    fn vanilla_religious_schools_muslim() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let data = load(&vfs, &loc, "religious_schools").unwrap();
        let h = data.objects.iter().find(|o| o.key == "hanafi_school").expect("hanafi_school");
        assert_eq!(h.group.as_deref(), Some("muslim"));
        assert!(h.script_blocks.iter().find(|s| s.name == "can_invite_scholar").unwrap().present);
        assert!(data.objects.len() >= 6, "expect at least the six muslim schools, got {}", data.objects.len());
    }

    #[test]
    fn vanilla_deities_and_reforms() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let deities = load(&vfs, &loc, "personal_deities").unwrap();
        let odin = deities.objects.iter().find(|o| o.key == "odin").expect("odin");
        assert!(odin.self_rows.iter().any(|r| r.key == "legitimacy"));
        let reforms = load(&vfs, &loc, "religious_reforms").unwrap();
        let nah = reforms.objects.iter().find(|o| o.key == "nahuatl_reforms").expect("nahuatl_reforms");
        assert!(nah.ordered_children.len() >= 4, "nahuatl reforms should have several steps");
    }

    // ---- Sprint 27 Wave 1: government reforms --------------------------------

    const GOV_REFORM_SRC: &str = "\
despotic_monarchy = {\n\
\ticon = \"crown\"\n\
\tallow_normal_conversion = yes\n\
\tlegacy_government = yes\n\
\tvalid_for_new_country = yes\n\
\tnation_designer_cost = 0\n\
\tmonarchy = yes\n\
\tmodifiers = {\n\t\tglobal_unrest = -1\n\t\tmax_absolutism = 5\n\t}\n\
\tcustom_attributes = {\n\t\tlocked_government_type = yes\n\t}\n\
\tconditional = {\n\t\tallow = { always = yes }\n\t}\n\
\tai = {\n\t\tfactor = 1\n\t}\n\
}\n";

    #[test]
    fn parses_government_reform_typed_icon_bools_modifiers() {
        let (_root, vfs) = synthetic(
            "gov_reform",
            &[("common/government_reforms/00_test.txt", GOV_REFORM_SRC)],
        );
        let loc = LocStore::from_pairs(&[("despotic_monarchy", "Despotic Monarchy")]);
        let data = load(&vfs, &loc, "government_reforms").unwrap();
        let r = data.objects.iter().find(|o| o.key == "despotic_monarchy").unwrap();
        assert_eq!(r.name, "Despotic Monarchy");
        // Named icon (quotes stripped by parser).
        assert_eq!(r.icon_kind, "named");
        assert_eq!(r.icon.as_deref(), Some("crown"));
        // desc uses the <key>_desc pattern.
        assert_eq!(r.desc_key, "despotic_monarchy_desc");
        // Typed booleans.
        let sc = |k: &str| r.scalars.iter().find(|s| s.key == k).unwrap();
        assert!(sc("allow_normal_conversion").present && sc("allow_normal_conversion").value == "yes");
        assert!(sc("monarchy").present && sc("monarchy").value == "yes");
        assert!(!sc("valid_for_nation_designer").present);
        // `modifiers` + `custom_attributes` are flat modifier blocks.
        let m = r.modifier_blocks.iter().find(|m| m.name == "modifiers").unwrap();
        assert!(m.present && m.flat);
        assert!(m.rows.iter().any(|x| x.key == "max_absolutism" && x.value == "5"));
        let ca = r.modifier_blocks.iter().find(|m| m.name == "custom_attributes").unwrap();
        assert!(ca.present && ca.rows.iter().any(|x| x.key == "locked_government_type"));
        // `conditional` is unmodeled → preserved raw.
        assert!(r.raw_extra.contains(&"conditional".to_string()));

        // Deep round-trip: a scalar bool + the modifiers block edit byte-surgically.
        let out = apply(
            GOV_REFORM_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["despotic_monarchy".into(), "monarchy".into()], value: "no".into(), quoted: false },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock { path: vec!["despotic_monarchy".into(), "modifiers".into()], value: "global_unrest = -2".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("monarchy = no"));
        assert!(text.contains("global_unrest = -2"));
        // Untouched content round-trips.
        assert!(text.contains("locked_government_type = yes"));
        assert!(text.contains("icon = \"crown\""));
    }

    #[test]
    fn government_reform_scaffold_has_required_keys() {
        let sc = scaffold("government_reforms", "my_reform", None).unwrap();
        let b = paradox::parse(&sc.text);
        let rb = b.get_block("my_reform").unwrap();
        assert!(rb.get_scalar("icon").is_some());
        assert!(rb.get_scalar("allow_normal_conversion").is_some());
        assert!(rb.get_block("modifiers").is_some());
        // reforms resolve their icon by name → no gfx side-effect.
        assert!(sc.gfx_file.is_none());
        assert!(sc.loc_entries.iter().any(|e| e.key == "my_reform_desc"));
    }

    // ---- Sprint 27 Wave 1: buildings -----------------------------------------

    const BUILDING_SRC: &str = "\
wharf = {\n\
\tcost = 100\n\
\ttime = 12\n\
\tmanufactory = {\n\t\tnaval_supplies\n\t\tfish\n\t\tsalt\n\t}\n\
\tonmap = yes\n\
\tmodifier = {\n\t\tlocal_production_efficiency = 0.5\n\t}\n\
\tmake_obsolete = trade_depot\n\
\ton_built = {\n\t\tadd_prosperity = 1\n\t}\n\
\tai_will_do = {\n\t\tfactor = 1\n\t}\n\
}\n";

    #[test]
    fn parses_building_manufactory_list_and_obsolete_ref() {
        let (_root, vfs) = synthetic("building", &[("common/buildings/00_test.txt", BUILDING_SRC)]);
        let loc = LocStore::from_pairs(&[("wharf", "Wharf")]);
        let data = load(&vfs, &loc, "buildings").unwrap();
        let w = data.objects.iter().find(|o| o.key == "wharf").unwrap();
        // Bare-token manufactory list.
        let mf = w.list_fields.iter().find(|l| l.name == "manufactory").unwrap();
        assert!(mf.present && mf.picker == "trade_good");
        assert_eq!(mf.tokens, vec!["naval_supplies", "fish", "salt"]);
        // make_obsolete is a bare (token) building reference with a picker.
        let mo = w.scalars.iter().find(|s| s.key == "make_obsolete").unwrap();
        assert!(mo.present && mo.value == "trade_depot" && mo.picker == "building" && mo.kind == "token");
        // onmap bool + cost/time ints.
        assert_eq!(w.scalars.iter().find(|s| s.key == "onmap").unwrap().value, "yes");
        assert_eq!(w.scalars.iter().find(|s| s.key == "cost").unwrap().value, "100");
        // manufactory is structural (not raw_extra).
        assert!(!w.raw_extra.contains(&"manufactory".to_string()));

        // Deep round-trip: rewrite the manufactory list byte-surgically.
        let out = apply(
            BUILDING_SRC.as_bytes(),
            &Edit::SetBlock { path: vec!["wharf".into(), "manufactory".into()], value: "coal iron".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("coal iron") || text.contains("coal\n") || text.contains("coal iron"));
        assert!(text.contains("make_obsolete = trade_depot"));
        assert!(text.contains("local_production_efficiency = 0.5"));
    }

    #[test]
    fn building_scaffold_emits_named_gfx_sprite() {
        let sc = scaffold("buildings", "my_building", None).unwrap();
        let b = paradox::parse(&sc.text);
        let bb = b.get_block("my_building").unwrap();
        assert!(bb.get_scalar("cost").is_some());
        assert!(bb.get_block("modifier").is_some());
        // The building resolves its icon by name → a gfx spriteType is emitted.
        assert_eq!(sc.gfx_file.as_deref(), Some("interface/zz_eutoolkit_buildings.gfx"));
        let gfx = sc.gfx_text.expect("gfx text");
        let gb = paradox::parse(&gfx);
        let sprites = gb.get_block("spriteTypes").expect("spriteTypes block");
        let st = sprites.get_block("spriteType").expect("spriteType");
        assert_eq!(st.get_scalar("name"), Some("GFX_my_building"));
        assert!(st.get_scalar("texturefile").unwrap().contains("building_default.tga"));
    }

    // ---- Sprint 27 Wave 1: institutions --------------------------------------

    const INSTITUTION_SRC: &str = "\
renaissance = {\n\
\tbonus = {\n\t\tdevelopment_cost = -0.05\n\t}\n\
\ttrade_company_efficiency = 0.4\n\
\thistorical_start_date = 1450.1.1\n\
\thistorical_start_province = 116\n\
\thistory = {\n\t\tis_year = 1450\n\t}\n\
\tcan_start = {\n\t\tis_year = 1450\n\t}\n\
\tstart_chance = 5\n\
\ton_start = institution_events.2\n\
\tcan_embrace = {\n\t\towner = { has_institution = feudalism }\n\t}\n\
\tembracement_speed = {\n\t\tmodifier = { factor = 1 always = yes }\n\t}\n\
\tai_will_do = {\n\t\tfactor = 24\n\t}\n\
}\n";

    #[test]
    fn parses_institution_origin_province_weight_and_event_ref() {
        let (_root, vfs) = synthetic("institution", &[("common/institutions/00_test.txt", INSTITUTION_SRC)]);
        let loc = LocStore::from_pairs(&[("renaissance", "Renaissance")]);
        let data = load(&vfs, &loc, "institutions").unwrap();
        let r = data.objects.iter().find(|o| o.key == "renaissance").unwrap();
        // The origin-province weight: historical_start_province (province picker) +
        // start_chance (the "in 100" weight).
        let hsp = r.scalars.iter().find(|s| s.key == "historical_start_province").unwrap();
        assert!(hsp.present && hsp.value == "116" && hsp.picker == "province" && hsp.kind == "int");
        assert_eq!(r.scalars.iter().find(|s| s.key == "start_chance").unwrap().value, "5");
        // historical_start_date is a bare token (unquoted date).
        let hsd = r.scalars.iter().find(|s| s.key == "historical_start_date").unwrap();
        assert!(hsd.value == "1450.1.1" && hsd.kind == "token");
        // `bonus` is the modifier block.
        let m = r.modifier_blocks.iter().find(|m| m.name == "bonus").unwrap();
        assert!(m.present && m.rows.iter().any(|x| x.key == "development_cost"));
        // on_start references an event.
        assert_eq!(r.event_refs.iter().find(|e| e.key == "on_start").unwrap().id, "institution_events.2");
        // embracement_speed + history are script blocks.
        assert!(r.script_blocks.iter().find(|s| s.name == "embracement_speed").unwrap().present);
        assert!(r.script_blocks.iter().find(|s| s.name == "history").unwrap().present);

        // Deep round-trip: retarget the origin province byte-surgically.
        let out = apply(
            INSTITUTION_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["renaissance".into(), "historical_start_province".into()], value: "50".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("historical_start_province = 50"));
        assert!(text.contains("on_start = institution_events.2"));
        assert!(text.contains("trade_company_efficiency = 0.4"));
    }

    #[test]
    fn institution_scaffold_emits_named_gfx_and_required_keys() {
        let sc = scaffold("institutions", "my_institution", None).unwrap();
        let b = paradox::parse(&sc.text);
        let ib = b.get_block("my_institution").unwrap();
        assert!(ib.get_block("bonus").is_some());
        assert!(ib.get_scalar("historical_start_province").is_some());
        assert!(ib.get_scalar("start_chance").is_some());
        // Institutions resolve their icon by name → a gfx spriteType is emitted.
        assert_eq!(sc.gfx_file.as_deref(), Some("interface/zz_eutoolkit_institutions.gfx"));
        let gfx = paradox::parse(&sc.gfx_text.unwrap());
        let st = gfx.get_block("spriteTypes").unwrap().get_block("spriteType").unwrap();
        assert_eq!(st.get_scalar("name"), Some("GFX_icon_institution_my_institution"));
    }

    // ---- Vanilla Wave-1 parse -------------------------------------------------

    #[test]
    fn vanilla_wave1_families_load() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        // Government reforms: despotic_monarchy typed.
        let reforms = load(&vfs, &loc, "government_reforms").unwrap();
        let dm = reforms.objects.iter().find(|o| o.key == "despotic_monarchy").expect("despotic_monarchy");
        assert_eq!(dm.icon_kind, "named");
        assert!(dm.modifier_blocks.iter().find(|m| m.name == "modifiers").unwrap().present);
        // Buildings: wharf carries a manufactory list.
        let buildings = load(&vfs, &loc, "buildings").unwrap();
        let wharf = buildings.objects.iter().find(|o| o.key == "wharf").expect("wharf");
        assert!(wharf.list_fields.iter().find(|l| l.name == "manufactory").unwrap().present);
        let temple = buildings.objects.iter().find(|o| o.key == "temple").expect("temple");
        assert!(temple.modifier_blocks.iter().find(|m| m.name == "modifier").unwrap().present);
        // Institutions: renaissance origin province + weight.
        let insts = load(&vfs, &loc, "institutions").unwrap();
        let ren = insts.objects.iter().find(|o| o.key == "renaissance").expect("renaissance");
        assert_eq!(ren.scalars.iter().find(|s| s.key == "historical_start_province").unwrap().value, "116");
        assert!(ren.event_refs.iter().any(|e| e.key == "on_start"));
    }

    // ---- Sprint 27 Wave 2: governments ---------------------------------------

    const GOV_SRC: &str = "\
monarchy = {\n\
\tcolor = { 179 25 25 }\n\
\tbasic_reform = monarchy_mechanic\n\
\tlegacy_government = {\n\t\tdespotic_monarchy\n\t\tfeudal_monarchy\n\t}\n\
\treform_levels = {\n\t\tfeudalism_vs_autocracy = {\n\t\t\treforms = { feudalism_reform autocracy_reform }\n\t\t}\n\t}\n\
\texclusive_reforms = { parliamentary_reform english_monarchy }\n\
\texclusive_reforms = { states_general_reform mughal_government }\n\
}\n\
pre_dharma_mapping = {\n\
\tdespotic_monarchy = { government = monarchy }\n\
}\n";

    #[test]
    fn parses_government_color_basic_reform_and_legacy_list() {
        let (_root, vfs) = synthetic("gov", &[("common/governments/00_test.txt", GOV_SRC)]);
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc, "governments").unwrap();
        // pre_dharma_mapping is excluded — only the monarchy category shows.
        assert_eq!(data.objects.len(), 1, "pre_dharma_mapping must be excluded");
        let g = &data.objects[0];
        assert_eq!(g.key, "monarchy");
        assert_eq!(g.color, Some([179, 25, 25]));
        assert_eq!(g.scalars.iter().find(|s| s.key == "basic_reform").unwrap().value, "monarchy_mechanic");
        // legacy_government is a bare-token list.
        let lg = g.list_fields.iter().find(|l| l.name == "legacy_government").unwrap();
        assert!(lg.present);
        assert_eq!(lg.tokens, vec!["despotic_monarchy", "feudal_monarchy"]);
        // The nested reform_levels + repeated exclusive_reforms round-trip raw.
        assert!(g.raw_extra.contains(&"reform_levels".to_string()));
        assert!(g.raw_extra.contains(&"exclusive_reforms".to_string()));
        // Deep round-trip: the basic_reform token + legacy list edit byte-surgically.
        let out = apply(
            GOV_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["monarchy".into(), "basic_reform".into()], value: "republic_mechanic".into(), quoted: false },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock { path: vec!["monarchy".into(), "legacy_government".into()], value: "despotic_monarchy".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("basic_reform = republic_mechanic"));
        // Untouched deep content preserved.
        assert!(text.contains("feudalism_vs_autocracy"));
        assert!(text.contains("mughal_government"));
    }

    // ---- Sprint 27 Wave 2: subject types (forward-declaration de-dup) ---------

    const SUBJ_SRC: &str = "\
vassal = {}\n\
march = {}\n\
default = {\n\tsprite = GFX_icon_vassal\n\tjoins_overlords_wars = yes\n}\n\
vassal = {\n\
\tcopy_from = default\n\
\tsprite = GFX_subject_vassal\n\
\tis_potential_overlord = { always = no }\n\
\trelative_power_class = 1\n\
\tcan_be_annexed = yes\n\
\tjoins_overlords_wars = yes\n\
\tmodifier_subject = { land_morale = -0.1 }\n\
}\n\
march = {\n\
\tcopy_from = vassal\n\
\tis_march = yes\n\
\tmodifier_subject = { land_morale = 0.1 }\n\
}\n";

    #[test]
    fn subject_types_dedup_keeps_definition_and_edit_key_addresses_it() {
        let (_root, vfs) = synthetic("subj", &[("common/subject_types/00_test.txt", SUBJ_SRC)]);
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc, "subject_types").unwrap();
        // `default` excluded; vassal/march de-duplicated to their rich definitions.
        assert_eq!(data.objects.len(), 2, "default excluded, vassal+march de-duped");
        let v = data.objects.iter().find(|o| o.key == "vassal").unwrap();
        // The definition (occurrence #1) wins over the forward declaration.
        assert_eq!(v.edit_key, "vassal#1");
        // copy_from is a subject-type reference (picker).
        let cf = v.scalars.iter().find(|s| s.key == "copy_from").unwrap();
        assert!(cf.present && cf.value == "default" && cf.picker == "subject_type");
        assert_eq!(v.scalars.iter().find(|s| s.key == "relative_power_class").unwrap().value, "1");
        // The many boolean properties are editable flat rows.
        assert!(v.self_modifier);
        assert!(v.self_rows.iter().any(|r| r.key == "can_be_annexed"));
        assert!(v.self_rows.iter().any(|r| r.key == "joins_overlords_wars"));
        // modifier_subject is a flat modifier block.
        let m = v.modifier_blocks.iter().find(|m| m.name == "modifier_subject").unwrap();
        assert!(m.present && m.rows.iter().any(|x| x.key == "land_morale"));
        // is_potential_overlord is a script trigger.
        assert!(v.script_blocks.iter().find(|s| s.name == "is_potential_overlord").unwrap().present);
        // raw span resolves the DEFINITION, not the empty forward declaration.
        assert!(v.raw.contains("GFX_subject_vassal"), "raw is the definition block: {}", v.raw);

        // Deep round-trip: editing via the occurrence-qualified edit_key touches
        // only the real definition, leaving the forward declaration untouched.
        let out = apply(
            SUBJ_SRC.as_bytes(),
            &Edit::SetScalar { path: vec![v.edit_key.clone(), "relative_power_class".into()], value: "2".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("relative_power_class = 2"));
        // The forward declaration `vassal = {}` is intact.
        assert!(text.contains("vassal = {}"));
        assert!(text.contains("GFX_subject_vassal"));
    }

    // ---- Sprint 27 Wave 2: subject type upgrades -----------------------------

    const SUBJ_UP_SRC: &str = "\
increase_force_limit_from_colony = {\n\
\tcan_upgrade_trigger = { is_subject_of_type = crown_colony }\n\
\tcost = 100\n\
\teffect = { colonial_parent = { adm_power_cost = 25 } }\n\
\tmodifier_overlord = { land_forcelimit = 5 }\n\
\tmodifier_subject = { land_forcelimit = -5 liberty_desire = 10 }\n\
}\n";

    #[test]
    fn parses_subject_type_upgrade_cost_modifiers_and_trigger() {
        let (_root, vfs) =
            synthetic("subjup", &[("common/subject_type_upgrades/00_test.txt", SUBJ_UP_SRC)]);
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc, "subject_type_upgrades").unwrap();
        let u = data.objects.iter().find(|o| o.key == "increase_force_limit_from_colony").unwrap();
        assert_eq!(u.scalars.iter().find(|s| s.key == "cost").unwrap().value, "100");
        let mo = u.modifier_blocks.iter().find(|m| m.name == "modifier_overlord").unwrap();
        assert!(mo.present && mo.rows.iter().any(|x| x.key == "land_forcelimit"));
        let ms = u.modifier_blocks.iter().find(|m| m.name == "modifier_subject").unwrap();
        assert!(ms.present && ms.rows.iter().any(|x| x.key == "liberty_desire" && x.value == "10"));
        assert!(u.script_blocks.iter().find(|s| s.name == "can_upgrade_trigger").unwrap().present);
        // Deep round-trip: rewrite the subject modifier block.
        let out = apply(
            SUBJ_UP_SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["increase_force_limit_from_colony".into(), "modifier_subject".into()],
                value: "liberty_desire = 20".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("liberty_desire = 20"));
        assert!(text.contains("land_forcelimit = 5"), "overlord modifier untouched");
    }

    // ---- Sprint 27 Wave 2: CB types ------------------------------------------

    const CB_SRC: &str = "\
cb_conquest = {\n\
\tvalid_for_subject = no\n\
\tis_triggered_only = no\n\
\tprerequisites = {\n\t\tFROM = { NOT = { num_of_cities = 1 } }\n\t}\n\
\tprerequisites_self = {\n\t\tis_subject = no\n\t}\n\
\twar_goal = take_claim\n\
\tattacker_disabled_po = { po_gold po_trade_power }\n\
}\n";

    #[test]
    fn parses_cb_type_war_goal_picker_and_prereqs() {
        let (_root, vfs) = synthetic("cb", &[("common/cb_types/00_test.txt", CB_SRC)]);
        let loc = LocStore::from_pairs(&[("cb_conquest", "Conquest")]);
        let data = load(&vfs, &loc, "cb_types").unwrap();
        let c = data.objects.iter().find(|o| o.key == "cb_conquest").unwrap();
        assert_eq!(c.name, "Conquest");
        // war_goal links to a wargoal_types entry (picker).
        let wg = c.scalars.iter().find(|s| s.key == "war_goal").unwrap();
        assert!(wg.present && wg.value == "take_claim" && wg.picker == "wargoal_type" && wg.kind == "token");
        // valid_for_subject/is_triggered_only bools.
        assert_eq!(c.scalars.iter().find(|s| s.key == "valid_for_subject").unwrap().value, "no");
        // prerequisites triggers present.
        assert!(c.script_blocks.iter().find(|s| s.name == "prerequisites").unwrap().present);
        assert!(c.script_blocks.iter().find(|s| s.name == "prerequisites_self").unwrap().present);
        // attacker_disabled_po is a bare-token list.
        let dp = c.list_fields.iter().find(|l| l.name == "attacker_disabled_po").unwrap();
        assert_eq!(dp.tokens, vec!["po_gold", "po_trade_power"]);
        // Deep round-trip: retarget the war goal byte-surgically.
        let out = apply(
            CB_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["cb_conquest".into(), "war_goal".into()], value: "take_province".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("war_goal = take_province"));
        assert!(text.contains("is_subject = no"), "prerequisites_self untouched");
    }

    // ---- Sprint 27 Wave 2: war goal types + peace-option blocks --------------

    const WG_SRC: &str = "\
take_claim = {\n\
\ttype = take_province\n\
\twar_name = ACQUIRE_WARNAME\n\
\tallowed_provinces = { is_claim = yes }\n\
\trequired_treaty_to_take_provinces = po_establish_eyalet\n\
\tattacker = {\n\
\t\tbadboy_factor = 1\n\
\t\tprestige_factor = 2\n\
\t\tpeace_cost_factor = 0.75\n\
\t\tpeace_options = { po_demand_provinces po_gold }\n\
\t}\n\
\tdefender = {\n\
\t\tbadboy_factor = 1\n\
\t\tpeace_options = { po_demand_provinces }\n\
\t}\n\
}\n";

    #[test]
    fn parses_wargoal_type_enum_treaty_picker_and_peace_option_blocks() {
        let (_root, vfs) = synthetic("wg", &[("common/wargoal_types/00_test.txt", WG_SRC)]);
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc, "wargoal_types").unwrap();
        let w = data.objects.iter().find(|o| o.key == "take_claim").unwrap();
        // type is an enum.
        let ty = w.scalars.iter().find(|s| s.key == "type").unwrap();
        assert!(ty.present && ty.value == "take_province" && ty.kind == "enum");
        assert!(ty.options.contains(&"take_region".to_string()));
        // required_treaty_to_take_provinces links to a peace treaty (picker).
        let rt = w.scalars.iter().find(|s| s.key == "required_treaty_to_take_provinces").unwrap();
        assert!(rt.present && rt.value == "po_establish_eyalet" && rt.picker == "peace_treaty");
        // allowed_provinces trigger present.
        assert!(w.script_blocks.iter().find(|s| s.name == "allowed_provinces").unwrap().present);
        // attacker/defender carry po_* peace options — non-flat, shown read-only.
        let atk = w.modifier_blocks.iter().find(|m| m.name == "attacker").unwrap();
        assert!(atk.present && !atk.flat, "peace-option block must be non-flat (read-only)");
        // Deep round-trip: enum + trigger edit; peace-option blocks round-trip untouched.
        let out = apply(
            WG_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["take_claim".into(), "type".into()], value: "take_capital".into(), quoted: false },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock { path: vec!["take_claim".into(), "allowed_provinces".into()], value: "is_core = yes".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("type = take_capital"));
        assert!(text.contains("is_core = yes"));
        // The po_* peace options survive verbatim.
        assert!(text.contains("po_demand_provinces po_gold"));
        assert!(text.contains("prestige_factor = 2"));
    }

    // ---- Sprint 27 Wave 2: peace treaties ------------------------------------

    const PT_SRC: &str = "\
po_establish_eyalet = {\n\
\tcategory = 6\n\
\tpower_projection = humiliated_rival\n\
\tpower_cost_base = 1.0\n\
\tprestige_base = 1.0\n\
\tae_base = 0.4\n\
\twarscore_cost = { all_provinces = 0.75 no_provinces = 0.0 }\n\
\twarscore_cap = 60\n\
\trequires_demand_independence = yes\n\
\tis_make_subject = yes\n\
\tis_visible = { always = yes }\n\
\tis_allowed = { religion = catholic }\n\
\teffect = { create_subject = { who = FROM subject_type = eyalet } }\n\
\tai_weight = { export_to_variable = { variable_name = ai_value value = 50 } }\n\
}\n";

    #[test]
    fn parses_peace_treaty_typed_warscore_cost_and_desc_suffix() {
        let (_root, vfs) = synthetic("pt", &[("common/peace_treaties/00_test.txt", PT_SRC)]);
        let loc = LocStore::from_pairs(&[("po_establish_eyalet", "Establish Eyalet")]);
        let data = load(&vfs, &loc, "peace_treaties").unwrap();
        let p = data.objects.iter().find(|o| o.key == "po_establish_eyalet").unwrap();
        assert_eq!(p.name, "Establish Eyalet");
        // desc uses the <key>_desc pattern.
        assert_eq!(p.desc_key, "po_establish_eyalet_desc");
        assert_eq!(p.scalars.iter().find(|s| s.key == "category").unwrap().value, "6");
        assert_eq!(p.scalars.iter().find(|s| s.key == "warscore_cap").unwrap().value, "60");
        assert_eq!(p.scalars.iter().find(|s| s.key == "is_make_subject").unwrap().value, "yes");
        // warscore_cost is a flat modifier block.
        let wc = p.modifier_blocks.iter().find(|m| m.name == "warscore_cost").unwrap();
        assert!(wc.present && wc.flat && wc.rows.iter().any(|r| r.key == "all_provinces" && r.value == "0.75"));
        // is_visible/is_allowed triggers + effect/ai_weight effects.
        assert!(p.script_blocks.iter().find(|s| s.name == "is_allowed").unwrap().present);
        assert!(p.script_blocks.iter().find(|s| s.name == "ai_weight").unwrap().present);
        // Deep round-trip.
        let out = apply(
            PT_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["po_establish_eyalet".into(), "warscore_cap".into()], value: "40".into(), quoted: false },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock { path: vec!["po_establish_eyalet".into(), "warscore_cost".into()], value: "all_provinces = 1.0".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("warscore_cap = 40"));
        assert!(text.contains("all_provinces = 1.0"));
        assert!(text.contains("subject_type = eyalet"), "effect block untouched");
    }

    // ---- Sprint 27 Wave 2: policies (self-modifier) --------------------------

    const POLICY_SRC: &str = "\
the_combination_act = {\n\
\tmonarch_power = ADM\n\
\tpotential = { has_idea_group = aristocracy_ideas }\n\
\tallow = { full_idea_group = aristocracy_ideas }\n\
\tproduction_efficiency = 0.20\n\
\tglobal_tax_modifier = 0.1\n\
\teffect = {}\n\
\tremoved_effect = {}\n\
\tai_will_do = { factor = 1 }\n\
}\n";

    #[test]
    fn parses_policy_monarch_power_and_flat_modifiers() {
        let (_root, vfs) = synthetic("policy", &[("common/policies/00_adm.txt", POLICY_SRC)]);
        let loc = LocStore::from_pairs(&[("the_combination_act", "The Combination Act")]);
        let data = load(&vfs, &loc, "policies").unwrap();
        let p = data.objects.iter().find(|o| o.key == "the_combination_act").unwrap();
        let mp = p.scalars.iter().find(|s| s.key == "monarch_power").unwrap();
        assert!(mp.present && mp.value == "ADM" && mp.kind == "enum");
        // Flat top-level modifiers are editable rows; the script blocks stay structural.
        assert!(p.self_modifier);
        assert!(p.self_rows.iter().any(|r| r.key == "production_efficiency" && r.value == "0.20"));
        assert!(p.self_rows.iter().any(|r| r.key == "global_tax_modifier"));
        assert!(!p.self_rows.iter().any(|r| r.key == "monarch_power"), "monarch_power is a structural scalar");
        assert!(p.script_blocks.iter().find(|s| s.name == "allow").unwrap().present);
        // Deep round-trip: a per-row modifier edit is byte-surgical.
        let out = apply(
            POLICY_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["the_combination_act".into(), "production_efficiency".into()], value: "0.25".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("production_efficiency = 0.25"));
        assert!(text.contains("global_tax_modifier = 0.1"));
    }

    // ---- Sprint 27 Wave 2: power projection ----------------------------------

    #[test]
    fn parses_power_projection_scalars() {
        const PP_SRC: &str = "\
great_power_1 = { power = 25 }\n\
subsidies_to_enemy_of_rival = { power = 50 max = 50 }\n\
embargoing_rival = { power = 3 max = 10 }\n";
        let (_root, vfs) = synthetic("pp", &[("common/powerprojection/00_static.txt", PP_SRC)]);
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc, "powerprojection").unwrap();
        let s = data.objects.iter().find(|o| o.key == "subsidies_to_enemy_of_rival").unwrap();
        assert_eq!(s.scalars.iter().find(|x| x.key == "power").unwrap().value, "50");
        assert_eq!(s.scalars.iter().find(|x| x.key == "max").unwrap().value, "50");
        let out = apply(
            PP_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["embargoing_rival".into(), "power".into()], value: "5".into(), quoted: false },
        )
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("power = 5"));
    }

    // ---- Sprint 27 Wave 2: vanilla loads -------------------------------------

    #[test]
    fn vanilla_wave2_families_load() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        // Governments: the five categories, pre_dharma_mapping excluded.
        let govs = load(&vfs, &loc, "governments").unwrap();
        assert!(govs.objects.iter().any(|o| o.key == "monarchy"));
        assert!(!govs.objects.iter().any(|o| o.key == "pre_dharma_mapping"));
        let mon = govs.objects.iter().find(|o| o.key == "monarchy").unwrap();
        assert!(mon.color.is_some());
        assert!(mon.list_fields.iter().find(|l| l.name == "legacy_government").unwrap().present);
        // Subject types: vassal de-duped to its definition, addressed as vassal#1.
        let subj = load(&vfs, &loc, "subject_types").unwrap();
        let vassal = subj.objects.iter().find(|o| o.key == "vassal").expect("vassal");
        assert!(vassal.edit_key.contains('#'), "vassal edit_key must be occurrence-qualified: {}", vassal.edit_key);
        assert!(vassal.raw.len() > 10 && !vassal.raw.contains("= {}"), "vassal raw is the definition");
        assert!(!subj.objects.iter().any(|o| o.key == "default"));
        // CB types: cb_conquest links a war goal.
        let cbs = load(&vfs, &loc, "cb_types").unwrap();
        let conq = cbs.objects.iter().find(|o| o.key == "cb_conquest").expect("cb_conquest");
        assert!(conq.scalars.iter().find(|s| s.key == "war_goal").unwrap().present);
        // War goal types: take_claim has attacker/defender peace-option blocks.
        let wgs = load(&vfs, &loc, "wargoal_types").unwrap();
        let tc = wgs.objects.iter().find(|o| o.key == "take_claim").expect("take_claim");
        assert!(tc.modifier_blocks.iter().find(|m| m.name == "attacker").unwrap().present);
        // Peace treaties: the scripted make_dummy example exists.
        let pts = load(&vfs, &loc, "peace_treaties").unwrap();
        assert!(!pts.objects.is_empty());
        // Policies: self-modifier rows.
        let pols = load(&vfs, &loc, "policies").unwrap();
        assert!(pols.objects.iter().any(|o| o.self_rows.iter().any(|r| !r.key.is_empty())));
        // Power projection: scalar-only entries.
        let pp = load(&vfs, &loc, "powerprojection").unwrap();
        assert!(pp.objects.iter().any(|o| o.scalars.iter().any(|s| s.key == "power" && s.present)));
    }

    // ---- Anbennar smoke ------------------------------------------------------

    #[test]
    fn anbennar_mechanics_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let mut total_mod = 0;
        for fam in FAMILIES {
            let data = load(&vfs, &loc, fam.id).unwrap();
            let modc = data.objects.iter().filter(|o| o.origin == "mod").count();
            total_mod += modc;
            // Every object's raw span parses back (no panic already asserted by load).
            println!("[mechanics:anbennar] {}: {} objects ({} mod)", fam.id, data.objects.len(), modc);
        }
        assert!(total_mod > 0, "Anbennar should contribute mod-origin mechanics content");

        // Round-trip a scalar edit on a heavily-customized family (Anbennar adds
        // custom disasters). Pick any mod disaster with a can_start block.
        let disasters = load(&vfs, &loc, "disasters").unwrap();
        if let Some(d) = disasters.objects.iter().find(|o| o.origin == "mod") {
            let bytes = vfs.read(&d.file).unwrap();
            // Editing an existing top-level trigger scalar must not panic; the
            // presence of the block is enough to assert the span resolves.
            assert!(mod_writer::block_span(&bytes, &[d.key.clone()]).is_some(), "mod disaster {} span resolves", d.key);
        }
    }

    // ---- Sprint 27 Wave 3: idea groups (category filter + ordered ideas) ------

    const IDEA_SRC: &str = "\
aristocracy_ideas = {\n\
\tcategory = MIL\n\
\tbonus = {\n\t\tleader_siege = 1\n\t}\n\
\ttrigger = {\n\t\thas_government_attribute = enables_aristocratic_idea_group\n\t}\n\
\tnoble_knights = {\n\t\tcavalry_power = 0.15\n\t}\n\
\tserfdom = {\n\t\tglobal_manpower_modifier = 0.15\n\t}\n\
\tai_will_do = {\n\t\tfactor = 3.75\n\t}\n\
}\n\
SWE_ideas = {\n\
\tstart = {\n\t\tland_morale = 0.1\n\t}\n\
\tbonus = {\n\t\tdiscipline = 0.05\n\t}\n\
\tswedish_idea_1 = {\n\t\tglobal_tax_modifier = 0.1\n\t}\n\
}\n";

    #[test]
    fn idea_groups_only_generic_with_category_and_ordered_ideas() {
        let (_root, vfs) = synthetic("ideas", &[("common/ideas/00_test.txt", IDEA_SRC)]);
        let loc = LocStore::from_pairs(&[("aristocracy_ideas", "Aristocratic Ideas")]);
        let data = load(&vfs, &loc, "idea_groups").unwrap();
        // SWE_ideas (no category) is excluded; only the generic group loads.
        assert_eq!(data.objects.len(), 1, "national idea sets (no category) excluded");
        let g = &data.objects[0];
        assert_eq!(g.key, "aristocracy_ideas");
        assert_eq!(g.scalars.iter().find(|s| s.key == "category").unwrap().value, "MIL");
        // bonus is a flat modifier block; start absent here.
        assert!(g.modifier_blocks.iter().find(|m| m.name == "bonus").unwrap().present);
        // The individual ideas are ordered children.
        assert!(g.ordered);
        let ideas: Vec<&str> = g.ordered_children.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(ideas, vec!["noble_knights", "serfdom"]);
        assert_eq!(g.ordered_children[0].rows[0].key, "cavalry_power");
        // trigger + ai_will_do are structural scripts, not children.
        assert!(g.script_blocks.iter().find(|s| s.name == "trigger").unwrap().present);
        // Deep round-trip: an idea modifier + the bonus block edit byte-surgically.
        let out = apply(
            IDEA_SRC.as_bytes(),
            &Edit::SetBlock { path: vec!["aristocracy_ideas".into(), "noble_knights".into()], value: "cavalry_power = 0.25".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("cavalry_power = 0.25"));
        assert!(text.contains("SWE_ideas"), "national set untouched");
    }

    // ---- advisor types (self-modifier + skill_scaled_modifier preserve) ------

    const ADVISOR_SRC: &str = "\
philosopher = {\n\
\tmonarch_power = ADM\n\
\tskill_scaled_modifier = {\n\t\ttrigger = { owner = { has_country_flag = x } }\n\t\tmodifier = { idea_cost = -0.01 }\n\t}\n\
\tprestige = 1\n\
\tskill_scaled_modifier = {\n\t\ttrigger = { always = yes }\n\t\tmodifier = { meritocracy = 0.25 }\n\t}\n\
\tai_will_do = { factor = 1 }\n\
}\n";

    #[test]
    fn advisor_type_monarch_power_flat_rows_and_preserved_skill_blocks() {
        let (_root, vfs) = synthetic("advisor", &[("common/advisortypes/00_test.txt", ADVISOR_SRC)]);
        let loc = LocStore::from_pairs(&[("philosopher", "Philosopher")]);
        let data = load(&vfs, &loc, "advisortypes").unwrap();
        let a = data.objects.iter().find(|o| o.key == "philosopher").unwrap();
        assert_eq!(a.name, "Philosopher");
        let mp = a.scalars.iter().find(|s| s.key == "monarch_power").unwrap();
        assert!(mp.present && mp.value == "ADM" && mp.kind == "enum");
        // prestige is a flat modifier row; skill_scaled_modifier blocks preserved.
        assert!(a.self_rows.iter().any(|r| r.key == "prestige" && r.value == "1"));
        assert!(a.raw_extra.contains(&"skill_scaled_modifier".to_string()));
        assert!(a.script_blocks.iter().find(|s| s.name == "ai_will_do").unwrap().present);
        // Deep round-trip: edit the flat row byte-surgically; skill blocks intact.
        let out = apply(
            ADVISOR_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["philosopher".into(), "prestige".into()], value: "2".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("prestige = 2"));
        assert!(text.contains("meritocracy = 0.25"), "skill_scaled_modifier preserved");
    }

    // ---- ruler personalities (self-modifier + allow/chance trees) ------------

    const RULER_PERS_SRC: &str = "\
just_personality = {\n\
\truler_allow = {\n\t\tallow = { NOT = { ruler_has_personality = cruel_personality } }\n\t\tchance = { modifier = { factor = 1 ADM = 6 } }\n\t}\n\
\their_allow = {\n\t\tchance = { modifier = { factor = 1 heir_ADM = 6 } }\n\t}\n\
\teasy_war_chance_multiplier = 0.5\n\
\tfair_fights = yes\n\
\tglobal_unrest = -2\n\
\tnation_designer_cost = 2\n\
}\n";

    #[test]
    fn ruler_personality_flat_rows_and_allow_trees() {
        let (_root, vfs) =
            synthetic("rulerpers", &[("common/ruler_personalities/00_test.txt", RULER_PERS_SRC)]);
        let loc = LocStore::from_pairs(&[("just_personality", "Just")]);
        let data = load(&vfs, &loc, "ruler_personalities").unwrap();
        let p = data.objects.iter().find(|o| o.key == "just_personality").unwrap();
        assert!(p.self_modifier);
        assert!(p.self_rows.iter().any(|r| r.key == "fair_fights" && r.value == "yes"));
        assert!(p.self_rows.iter().any(|r| r.key == "global_unrest"));
        // nation_designer_cost is a structural scalar (not a modifier row).
        assert!(!p.self_rows.iter().any(|r| r.key == "nation_designer_cost"));
        assert_eq!(p.scalars.iter().find(|s| s.key == "nation_designer_cost").unwrap().value, "2");
        // ruler_allow / heir_allow are script trees.
        assert!(p.script_blocks.iter().find(|s| s.name == "ruler_allow").unwrap().present);
        assert!(p.script_blocks.iter().find(|s| s.name == "heir_allow").unwrap().present);
        // Deep round-trip: a flat row edit; the allow trees survive.
        let out = apply(
            RULER_PERS_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["just_personality".into(), "easy_war_chance_multiplier".into()], value: "0.75".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("easy_war_chance_multiplier = 0.75"));
        assert!(text.contains("ruler_has_personality = cruel_personality"), "allow tree preserved");
    }

    // ---- modifier registries (event/opinion/static/triggered) ----------------

    #[test]
    fn modifier_registries_self_modifier_rows_and_triggers() {
        const EVMOD: &str = "the_proper_old_ways = {\n\traze_power_gain = 0.2\n\tland_morale = 0.1\n}\n";
        let (_root, vfs) = synthetic("evmod", &[("common/event_modifiers/00_test.txt", EVMOD)]);
        let loc = LocStore::from_pairs(&[]);
        let e = load(&vfs, &loc, "event_modifiers").unwrap();
        let m = e.objects.iter().find(|o| o.key == "the_proper_old_ways").unwrap();
        assert!(m.self_modifier && m.self_rows.iter().any(|r| r.key == "land_morale"));

        const OPMOD: &str = "enemy_of_my_enemy = {\n\topinion = 1\n\tmax = 20\n}\n";
        let (_r2, vfs2) = synthetic("opmod", &[("common/opinion_modifiers/00_test.txt", OPMOD)]);
        let o = load(&vfs2, &loc, "opinion_modifiers").unwrap();
        let om = o.objects.iter().find(|x| x.key == "enemy_of_my_enemy").unwrap();
        assert!(om.self_rows.iter().any(|r| r.key == "opinion" && r.value == "1"));

        const TRMOD: &str = "east_indian_trade_route = {\n\tpotential = { num_of_ports = 1 }\n\ttrigger = { num_of_ports = 4 }\n\ttrade_efficiency = 0.05\n}\n";
        let (_r3, vfs3) = synthetic("trmod", &[("common/triggered_modifiers/00_test.txt", TRMOD)]);
        let t = load(&vfs3, &loc, "triggered_modifiers").unwrap();
        let tm = t.objects.iter().find(|x| x.key == "east_indian_trade_route").unwrap();
        assert!(tm.self_rows.iter().any(|r| r.key == "trade_efficiency"));
        assert!(tm.script_blocks.iter().find(|s| s.name == "potential").unwrap().present);
        assert!(tm.script_blocks.iter().find(|s| s.name == "trigger").unwrap().present);
        // Deep round-trip: byte-surgical flat-row edit on the triggered modifier.
        let out = apply(
            TRMOD.as_bytes(),
            &Edit::SetScalar { path: vec!["east_indian_trade_route".into(), "trade_efficiency".into()], value: "0.1".into(), quoted: false },
        )
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("trade_efficiency = 0.1"));
    }

    // ---- ages (objectives + abilities sub-groups + absolutism modifier) ------

    const AGE_SRC: &str = "\
age_of_absolutism = {\n\
\tstart = 1620\n\
\tcan_start = { is_institution_enabled = global_trade }\n\
\tpapacy = 1.5\n\
\tabsolutism = {\n\t\tharsh_treatment = 1\n\t\tstability = 1\n\t}\n\
\tobjectives = {\n\
\t\tobj_3_trade_companies = {\n\t\t\tnum_of_trade_companies = 3\n\t\t}\n\
\t\tobj_universities = {\n\t\t\tnum_of_owned_provinces_with = { has_building = university value = 5 }\n\t\t}\n\
\t}\n\
\tabilities = {\n\
\t\tab_yearly_absolutism = {\n\t\t\teffect = { on_age_ability_taken = { age = age_of_absolutism } }\n\t\t\tmodifier = { yearly_absolutism = 1 }\n\t\t\tai_will_do = { factor = 5 }\n\t\t}\n\
\t}\n\
}\n";

    #[test]
    fn age_objectives_and_abilities_modeled_as_sub_entries() {
        let (_root, vfs) = synthetic("age", &[("common/ages/00_test.txt", AGE_SRC)]);
        let loc = LocStore::from_pairs(&[("age_of_absolutism", "Age of Absolutism")]);
        let data = load(&vfs, &loc, "ages").unwrap();
        let a = data.objects.iter().find(|o| o.key == "age_of_absolutism").unwrap();
        assert_eq!(a.scalars.iter().find(|s| s.key == "start").unwrap().value, "1620");
        assert_eq!(a.scalars.iter().find(|s| s.key == "papacy").unwrap().value, "1.5");
        // absolutism is a flat modifier block.
        let abs = a.modifier_blocks.iter().find(|m| m.name == "absolutism").unwrap();
        assert!(abs.present && abs.flat && abs.rows.iter().any(|r| r.key == "harsh_treatment"));
        // Two sub-groups: objectives (trigger children) + abilities (typed).
        let objectives = a.sub_groups.iter().find(|g| g.container == "objectives").unwrap();
        assert!(objectives.child_is_trigger);
        let obj_keys: Vec<&str> = objectives.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(obj_keys, vec!["obj_3_trade_companies", "obj_universities"]);
        let abilities = a.sub_groups.iter().find(|g| g.container == "abilities").unwrap();
        assert!(!abilities.child_is_trigger);
        let ab = abilities.entries.iter().find(|e| e.key == "ab_yearly_absolutism").unwrap();
        // The ability carries a flat modifier block + effect / ai_will_do scripts.
        assert!(ab.modifier_blocks.iter().find(|m| m.name == "modifier").unwrap().rows.iter().any(|r| r.key == "yearly_absolutism"));
        assert!(ab.script_blocks.iter().find(|s| s.name == "effect").unwrap().present);
        assert!(ab.script_blocks.iter().find(|s| s.name == "ai_will_do").unwrap().present);
        // objectives/abilities are structural, not raw_extra.
        assert!(!a.raw_extra.contains(&"objectives".to_string()));

        // Deep round-trip: edit an objective (trigger child) + an ability modifier.
        let out = apply(
            AGE_SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["age_of_absolutism".into(), "objectives".into(), "obj_3_trade_companies".into()],
                value: "num_of_trade_companies = 5".into(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock {
                path: vec!["age_of_absolutism".into(), "abilities".into(), "ab_yearly_absolutism".into(), "modifier".into()],
                value: "yearly_absolutism = 2".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("num_of_trade_companies = 5"));
        assert!(text.contains("yearly_absolutism = 2"));
        assert!(text.contains("is_institution_enabled = global_trade"), "can_start untouched");
    }

    // ---- hegemons + state edicts + government ranks + natives -----------------

    #[test]
    fn hegemon_state_edict_rank_and_native_shapes() {
        const HEG: &str = "economic_hegemon = {\n\tallow = { is_great_power = yes }\n\tbase = { war_exhaustion = -0.1 }\n\tscale = { global_trade_goods_size_modifier = 0.25 }\n\tmax = { governing_capacity_modifier = 0.2 }\n}\n";
        let (_r, vfs) = synthetic("heg", &[("common/hegemons/00_test.txt", HEG)]);
        let loc = LocStore::from_pairs(&[]);
        let h = load(&vfs, &loc, "hegemons").unwrap();
        let he = h.objects.iter().find(|o| o.key == "economic_hegemon").unwrap();
        for m in ["base", "scale", "max"] {
            assert!(he.modifier_blocks.iter().find(|x| x.name == m).unwrap().present, "hegemon {m}");
        }
        assert!(he.script_blocks.iter().find(|s| s.name == "allow").unwrap().present);

        const SE: &str = "edict_protect_trade = {\n\tpotential = { always = yes }\n\tallow = { always = yes }\n\tmodifier = { province_trade_power_modifier = 0.5 }\n\tcolor = { 113 11 43 }\n\tai_will_do = { factor = 10 }\n}\n";
        let (_r2, vfs2) = synthetic("sedict", &[("common/state_edicts/00_test.txt", SE)]);
        let s = load(&vfs2, &loc, "state_edicts").unwrap();
        let se = s.objects.iter().find(|o| o.key == "edict_protect_trade").unwrap();
        assert_eq!(se.color, Some([113, 11, 43]));
        assert!(se.modifier_blocks.iter().find(|m| m.name == "modifier").unwrap().present);
        assert!(se.script_blocks.iter().find(|x| x.name == "ai_will_do").unwrap().present);

        const GR: &str = "3 = {\n\tgoverning_capacity = 400\n\tmax_absolutism = 5\n\tdiplomats = 1\n}\n";
        let (_r3, vfs3) = synthetic("grank", &[("common/government_ranks/00_test.txt", GR)]);
        let g = load(&vfs3, &loc, "government_ranks").unwrap();
        let gr = g.objects.iter().find(|o| o.key == "3").unwrap();
        assert!(gr.self_rows.iter().any(|r| r.key == "max_absolutism" && r.value == "5"));
        // Byte-surgical per-rank modifier edit (rank keyed by number).
        let out = apply(
            GR.as_bytes(),
            &Edit::SetScalar { path: vec!["3".into(), "max_absolutism".into()], value: "10".into(), quoted: false },
        )
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("max_absolutism = 10"));

        const NAT: &str = "natives_american_na = {\n\tgraphical_culture = northamericagfx\n\tcolor = { 0 255 0 }\n\ticon = 1\n\tunit = native_indian_archer\n\tprovinces = { 481 867 868 }\n}\n";
        let (_r4, vfs4) = synthetic("nat", &[("common/natives/00_test.txt", NAT)]);
        let n = load(&vfs4, &loc, "natives").unwrap();
        let na = n.objects.iter().find(|o| o.key == "natives_american_na").unwrap();
        assert_eq!(na.color, Some([0, 255, 0]));
        assert_eq!(na.scalars.iter().find(|s| s.key == "graphical_culture").unwrap().value, "northamericagfx");
        let pl = na.list_fields.iter().find(|l| l.name == "provinces").unwrap();
        assert_eq!(pl.tokens, vec!["481", "867", "868"]);
    }

    // ---- Wave 3 vanilla loads -------------------------------------------------

    #[test]
    fn vanilla_wave3_families_load() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        // Idea groups: only generic (category-bearing) groups; a national set absent.
        let ideas = load(&vfs, &loc, "idea_groups").unwrap();
        assert!(ideas.objects.iter().any(|o| o.key == "aristocracy_ideas"));
        assert!(!ideas.objects.iter().any(|o| o.key.ends_with("_ideas") && o.scalars.iter().all(|s| !(s.key == "category" && s.present))));
        assert!(ideas.objects.iter().find(|o| o.key == "aristocracy_ideas").unwrap().ordered_children.len() >= 5);
        // Advisor types: philosopher self-modifier + preserved skill blocks.
        let adv = load(&vfs, &loc, "advisortypes").unwrap();
        let phil = adv.objects.iter().find(|o| o.key == "philosopher").expect("philosopher");
        assert!(phil.self_rows.iter().any(|r| !r.key.is_empty()));
        assert!(phil.raw_extra.contains(&"skill_scaled_modifier".to_string()));
        // Personalities load.
        assert!(!load(&vfs, &loc, "ruler_personalities").unwrap().objects.is_empty());
        assert!(!load(&vfs, &loc, "leader_personalities").unwrap().objects.is_empty());
        // Modifier registries load.
        for fam in ["event_modifiers", "opinion_modifiers", "static_modifiers", "triggered_modifiers"] {
            assert!(!load(&vfs, &loc, fam).unwrap().objects.is_empty(), "{fam} empty");
        }
        // Ages: age_of_absolutism with objectives + abilities + absolutism.
        let ages = load(&vfs, &loc, "ages").unwrap();
        let aoa = ages.objects.iter().find(|o| o.key == "age_of_absolutism").expect("age_of_absolutism");
        assert!(aoa.modifier_blocks.iter().find(|m| m.name == "absolutism").unwrap().present);
        let objs = aoa.sub_groups.iter().find(|g| g.container == "objectives").unwrap();
        assert!(!objs.entries.is_empty(), "age has objectives");
        let abis = aoa.sub_groups.iter().find(|g| g.container == "abilities").unwrap();
        assert!(!abis.entries.is_empty(), "age has abilities");
        // Hegemons, state edicts, government ranks, natives.
        assert!(!load(&vfs, &loc, "hegemons").unwrap().objects.is_empty());
        assert!(!load(&vfs, &loc, "state_edicts").unwrap().objects.is_empty());
        let ranks = load(&vfs, &loc, "government_ranks").unwrap();
        assert!(ranks.objects.iter().any(|o| o.self_rows.iter().any(|r| r.key == "governing_capacity")));
        assert!(!load(&vfs, &loc, "natives").unwrap().objects.is_empty());
    }

    // ---- Sprint 27 Wave 4: trading policies ----------------------------------

    const TRADE_POLICY_SRC: &str = "\
maximize_profit = {\n\
\tpotential = { NOT = { has_country_flag = disabled_x } }\n\
\tcan_select = { FROM = { has_trader = ROOT } }\n\
\ttrade_power = { duration = -1 power_modifier = 0.05 key = maximize_profits }\n\
\tcenter_of_reformation = no\n\
\tbutton_gfx = GFX_Trading_Policy_Max_Profit\n\
\tcooldown = no\n\
}\n";

    #[test]
    fn parses_trading_policy_trade_power_block_and_button_gfx() {
        let (_root, vfs) =
            synthetic("tpolicy", &[("common/trading_policies/00_test.txt", TRADE_POLICY_SRC)]);
        let loc = LocStore::from_pairs(&[("maximize_profit", "Maximize Profit")]);
        let data = load(&vfs, &loc, "trading_policies").unwrap();
        let p = data.objects.iter().find(|o| o.key == "maximize_profit").unwrap();
        assert_eq!(p.name, "Maximize Profit");
        // button_gfx is a bare GFX token; center_of_reformation/cooldown bools.
        let bg = p.scalars.iter().find(|s| s.key == "button_gfx").unwrap();
        assert!(bg.present && bg.value == "GFX_Trading_Policy_Max_Profit" && bg.kind == "token");
        assert_eq!(p.scalars.iter().find(|s| s.key == "cooldown").unwrap().value, "no");
        // trade_power is a flat named block (duration/power_modifier/key rows).
        let tp = p.modifier_blocks.iter().find(|m| m.name == "trade_power").unwrap();
        assert!(tp.present && tp.flat && tp.rows.iter().any(|r| r.key == "power_modifier" && r.value == "0.05"));
        // triggers present.
        assert!(p.script_blocks.iter().find(|s| s.name == "can_select").unwrap().present);
        // Deep round-trip: retarget the button + trade_power block byte-surgically.
        let out = apply(
            TRADE_POLICY_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["maximize_profit".into(), "button_gfx".into()], value: "GFX_new".into(), quoted: false },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock { path: vec!["maximize_profit".into(), "trade_power".into()], value: "duration = -1 power_modifier = 0.1 key = maximize_profits".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("button_gfx = GFX_new"));
        assert!(text.contains("power_modifier = 0.1"));
        assert!(text.contains("has_trader = ROOT"), "can_select untouched");
    }

    // ---- Sprint 27 Wave 4: trade company investments -------------------------

    const TCI_SRC: &str = "\
local_quarter = {\n\
\tcategory = company_garrison\n\
\tsprite = \"GFX_investment_local_quarter\"\n\
\tupgrades_to = permanent_quarters\n\
\tcost = 200.0\n\
\tcompany_province_area_modifier = { local_defensiveness = 0.15 supply_limit_modifier = 0.25 }\n\
\tai_global_worth = { factor = 0 }\n\
\tai_area_worth = { factor = 1 }\n\
\tai_region_worth = { factor = 0 }\n\
}\n";

    #[test]
    fn parses_trade_company_investment_category_sprite_cost_and_area_modifier() {
        let (_root, vfs) = synthetic("tci", &[("common/tradecompany_investments/00_test.txt", TCI_SRC)]);
        let loc = LocStore::from_pairs(&[("local_quarter", "Local Quarter")]);
        let data = load(&vfs, &loc, "tradecompany_investments").unwrap();
        let i = data.objects.iter().find(|o| o.key == "local_quarter").unwrap();
        assert_eq!(i.scalars.iter().find(|s| s.key == "category").unwrap().value, "company_garrison");
        assert_eq!(i.scalars.iter().find(|s| s.key == "cost").unwrap().value, "200.0");
        assert_eq!(i.scalars.iter().find(|s| s.key == "upgrades_to").unwrap().value, "permanent_quarters");
        // sprite is a quoted named reference (parser strips quotes).
        let sp = i.scalars.iter().find(|s| s.key == "sprite").unwrap();
        assert!(sp.present && sp.value == "GFX_investment_local_quarter" && sp.kind == "str");
        // area modifier flat block.
        let m = i.modifier_blocks.iter().find(|m| m.name == "company_province_area_modifier").unwrap();
        assert!(m.present && m.flat && m.rows.iter().any(|r| r.key == "local_defensiveness"));
        // ai_*_worth weight blocks are scripts.
        assert!(i.script_blocks.iter().find(|s| s.name == "ai_area_worth").unwrap().present);
        // Deep round-trip: rewrite the area modifier + cost.
        let out = apply(
            TCI_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["local_quarter".into(), "cost".into()], value: "300.0".into(), quoted: false },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock { path: vec!["local_quarter".into(), "company_province_area_modifier".into()], value: "local_defensiveness = 0.3".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("cost = 300.0"));
        assert!(text.contains("local_defensiveness = 0.3"));
        assert!(text.contains("upgrades_to = permanent_quarters"));
    }

    // ---- Sprint 27 Wave 4: centers of trade ----------------------------------

    const COT_SRC: &str = "\
entrepot = {\n\
\tlevel = 2\n\
\tdevelopment = 10\n\
\tcost = 200\n\
\ttype = coastal\n\
\tprovince_modifiers = { province_trade_power_value = 10 local_development_cost = -0.05 }\n\
\tstate_modifiers = { local_development_cost = -0.1 }\n\
}\n";

    #[test]
    fn parses_center_of_trade_tier_and_type_enum() {
        let (_root, vfs) = synthetic("cot", &[("common/centers_of_trade/00_test.txt", COT_SRC)]);
        let loc = LocStore::from_pairs(&[("entrepot", "Entrepot")]);
        let data = load(&vfs, &loc, "centers_of_trade").unwrap();
        let c = data.objects.iter().find(|o| o.key == "entrepot").unwrap();
        assert_eq!(c.scalars.iter().find(|s| s.key == "level").unwrap().value, "2");
        assert_eq!(c.scalars.iter().find(|s| s.key == "cost").unwrap().value, "200");
        let ty = c.scalars.iter().find(|s| s.key == "type").unwrap();
        assert!(ty.present && ty.value == "coastal" && ty.kind == "enum" && ty.options.contains(&"inland".to_string()));
        let pm = c.modifier_blocks.iter().find(|m| m.name == "province_modifiers").unwrap();
        assert!(pm.present && pm.rows.iter().any(|r| r.key == "province_trade_power_value" && r.value == "10"));
        // Deep round-trip: bump the tier + rewrite province modifiers.
        let out = apply(
            COT_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["entrepot".into(), "level".into()], value: "3".into(), quoted: false },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock { path: vec!["entrepot".into(), "province_modifiers".into()], value: "province_trade_power_value = 25".into() },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("level = 3"));
        assert!(text.contains("province_trade_power_value = 25"));
        assert!(text.contains("local_development_cost = -0.1"), "state_modifiers untouched");
    }

    // ---- Sprint 27 Wave 4: naval doctrines / professionalism / flagship ------

    #[test]
    fn naval_doctrine_professionalism_and_flagship_shapes() {
        let loc = LocStore::from_pairs(&[]);

        const ND: &str = "fleet_in_being = {\n\tcan_select = { is_primitive = no }\n\tcost = 0.1\n\tcountry_modifier = { naval_maintenance_modifier = -0.15 }\n\teffect = {}\n\tremoved_effect = {}\n\tbutton_gfx = 1\n}\n";
        let (_r, vfs) = synthetic("nd", &[("common/naval_doctrines/00_test.txt", ND)]);
        let d = load(&vfs, &loc, "naval_doctrines").unwrap();
        let nd = d.objects.iter().find(|o| o.key == "fleet_in_being").unwrap();
        assert_eq!(nd.scalars.iter().find(|s| s.key == "cost").unwrap().value, "0.1");
        assert_eq!(nd.scalars.iter().find(|s| s.key == "button_gfx").unwrap().value, "1");
        assert!(nd.modifier_blocks.iter().find(|m| m.name == "country_modifier").unwrap().present);
        assert!(nd.script_blocks.iter().find(|s| s.name == "can_select").unwrap().present);
        let out = apply(
            ND.as_bytes(),
            &Edit::SetBlock { path: vec!["fleet_in_being".into(), "country_modifier".into()], value: "naval_maintenance_modifier = -0.2".into() },
        )
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("naval_maintenance_modifier = -0.2"));

        const PROF: &str = "supply_depot_modifier = {\n\tarmy_professionalism = 0.2\n\tmarker_sprite = GFX_pa_rank_1\n\tunit_sprite_start = \"GFX_ap2_\"\n\ttrigger = { always = yes }\n\tmay_build_supply_depot = yes\n\tdrill_gain_modifier = 0.1\n}\n";
        let (_r2, vfs2) = synthetic("prof", &[("common/professionalism/00_test.txt", PROF)]);
        let p = load(&vfs2, &loc, "professionalism").unwrap();
        let pr = p.objects.iter().find(|o| o.key == "supply_depot_modifier").unwrap();
        // The threshold + sprite refs are structural scalars, not modifier rows.
        assert_eq!(pr.scalars.iter().find(|s| s.key == "army_professionalism").unwrap().value, "0.2");
        assert_eq!(pr.scalars.iter().find(|s| s.key == "marker_sprite").unwrap().value, "GFX_pa_rank_1");
        assert!(pr.self_modifier);
        assert!(pr.self_rows.iter().any(|r| r.key == "may_build_supply_depot" && r.value == "yes"));
        assert!(pr.self_rows.iter().any(|r| r.key == "drill_gain_modifier"));
        assert!(!pr.self_rows.iter().any(|r| r.key == "army_professionalism"), "threshold is structural");
        assert!(pr.script_blocks.iter().find(|s| s.name == "trigger").unwrap().present);
        let out = apply(
            PROF.as_bytes(),
            &Edit::SetScalar { path: vec!["supply_depot_modifier".into(), "drill_gain_modifier".into()], value: "0.2".into(), quoted: false },
        )
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("drill_gain_modifier = 0.2"));

        const FLAG: &str = "mass_load_cannons = {\n\ttrigger = { normal_or_historical_nations = no }\n\tmodifier = { number_of_cannons_flagship_modifier = 0.50 naval_maintenance_flagship_modifier = 0.5 }\n\tai_trade_score = { factor = 0 }\n\tai_war_score = { factor = 1 }\n}\n";
        let (_r3, vfs3) = synthetic("flag", &[("common/flagship_modifications/00_test.txt", FLAG)]);
        let f = load(&vfs3, &loc, "flagship_modifications").unwrap();
        let fl = f.objects.iter().find(|o| o.key == "mass_load_cannons").unwrap();
        let m = fl.modifier_blocks.iter().find(|m| m.name == "modifier").unwrap();
        assert!(m.present && m.rows.iter().any(|r| r.key == "number_of_cannons_flagship_modifier"));
        assert!(fl.script_blocks.iter().find(|s| s.name == "trigger").unwrap().present);
        assert!(fl.script_blocks.iter().find(|s| s.name == "ai_war_score").unwrap().present);
        let out = apply(
            FLAG.as_bytes(),
            &Edit::SetBlock { path: vec!["mass_load_cannons".into(), "modifier".into()], value: "flagship_morale = 1".into() },
        )
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("flagship_morale = 1"));
    }

    // ---- Sprint 27 Wave 5: custom ideas / ai personalities / insults ---------

    const CUSTOM_IDEA_SRC: &str = "\
adm_idea_modifiers = {\n\
\tcategory = ADM\n\
\tcustom_idea_global_tax_modifier = {\n\t\tglobal_tax_modifier = 0.05\n\t\tlevel_cost_2 = 3\n\t\tdefault = 2\n\t\tchance = { factor = 1 }\n\t}\n\
\tcustom_idea_production_efficiency = {\n\t\tproduction_efficiency = 0.05\n\t\tdefault = 8\n\t\tchance = { factor = 1 }\n\t}\n\
}\n";

    #[test]
    fn parses_custom_idea_category_and_ordered_children() {
        let (_root, vfs) = synthetic("cidea", &[("common/custom_ideas/00_test.txt", CUSTOM_IDEA_SRC)]);
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc, "custom_ideas").unwrap();
        let c = data.objects.iter().find(|o| o.key == "adm_idea_modifiers").unwrap();
        let cat = c.scalars.iter().find(|s| s.key == "category").unwrap();
        assert!(cat.present && cat.value == "ADM" && cat.kind == "enum");
        // The per-idea children are ordered children (category is structural).
        assert!(c.ordered);
        let ideas: Vec<&str> = c.ordered_children.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(ideas, vec!["custom_idea_global_tax_modifier", "custom_idea_production_efficiency"]);
        // The modifier + level_cost rows are captured; the chance weight is non-flat.
        let first = &c.ordered_children[0];
        assert!(first.rows.iter().any(|r| r.key == "global_tax_modifier"));
        assert!(first.rows.iter().any(|r| r.key == "level_cost_2"));
        assert!(!first.flat, "child carries a chance weight → non-flat");
        // Deep round-trip: edit an idea's modifier byte-surgically.
        let out = apply(
            CUSTOM_IDEA_SRC.as_bytes(),
            &Edit::SetScalar { path: vec!["adm_idea_modifiers".into(), "custom_idea_global_tax_modifier".into(), "global_tax_modifier".into()], value: "0.1".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("global_tax_modifier = 0.1"));
        assert!(text.contains("production_efficiency = 0.05"), "sibling idea untouched");
    }

    #[test]
    fn parses_ai_personality_and_insult() {
        let loc = LocStore::from_pairs(&[]);

        const AIP: &str = "ai_capitalist = {\n\tchance = { factor = 100 modifier = { factor = 1.5 adm = 4 } }\n\ticon = 3\n}\n";
        let (_r, vfs) = synthetic("aip", &[("common/ai_personalities/00_test.txt", AIP)]);
        let d = load(&vfs, &loc, "ai_personalities").unwrap();
        let a = d.objects.iter().find(|o| o.key == "ai_capitalist").unwrap();
        assert_eq!(a.scalars.iter().find(|s| s.key == "icon").unwrap().value, "3");
        assert!(a.script_blocks.iter().find(|s| s.name == "chance").unwrap().present);
        // The chance weight round-trips on a byte-surgical icon edit.
        let out = apply(
            AIP.as_bytes(),
            &Edit::SetScalar { path: vec!["ai_capitalist".into(), "icon".into()], value: "5".into(), quoted: false },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("icon = 5"));
        assert!(text.contains("adm = 4"), "chance weight preserved");

        const INS: &str = "insult_default2 = {\n\ttrigger = { FROM = { religion_group = christian } }\n}\n";
        let (_r2, vfs2) = synthetic("ins", &[("common/insults/00_test.txt", INS)]);
        let i = load(&vfs2, &loc, "insults").unwrap();
        let ins = i.objects.iter().find(|o| o.key == "insult_default2").unwrap();
        assert!(ins.script_blocks.iter().find(|s| s.name == "trigger").unwrap().present);
        let out = apply(
            INS.as_bytes(),
            &Edit::SetBlock { path: vec!["insult_default2".into(), "trigger".into()], value: "always = yes".into() },
        )
        .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("always = yes"));
    }

    // ---- Sprint 27 Wave 4 + 5 vanilla loads ----------------------------------

    #[test]
    fn vanilla_wave4_and_wave5_families_load() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        // Wave 4.
        let tp = load(&vfs, &loc, "trading_policies").unwrap();
        let mp = tp.objects.iter().find(|o| o.key == "maximize_profit").expect("maximize_profit");
        assert!(mp.modifier_blocks.iter().find(|m| m.name == "trade_power").unwrap().present);
        let tci = load(&vfs, &loc, "tradecompany_investments").unwrap();
        assert!(tci.objects.iter().any(|o| o.scalars.iter().any(|s| s.key == "cost" && s.present)));
        let cot = load(&vfs, &loc, "centers_of_trade").unwrap();
        let sp = cot.objects.iter().find(|o| o.key == "staple_port").expect("staple_port");
        assert_eq!(sp.scalars.iter().find(|s| s.key == "type").unwrap().value, "coastal");
        let nd = load(&vfs, &loc, "naval_doctrines").unwrap();
        assert!(nd.objects.iter().any(|o| o.modifier_blocks.iter().any(|m| m.name == "country_modifier" && m.present)));
        let prof = load(&vfs, &loc, "professionalism").unwrap();
        assert!(prof.objects.iter().any(|o| o.scalars.iter().any(|s| s.key == "army_professionalism" && s.present)));
        let flag = load(&vfs, &loc, "flagship_modifications").unwrap();
        assert!(flag.objects.iter().any(|o| o.modifier_blocks.iter().any(|m| m.name == "modifier" && m.present)));
        // Wave 5.
        let ci = load(&vfs, &loc, "custom_ideas").unwrap();
        let adm = ci.objects.iter().find(|o| o.key == "adm_idea_modifiers").expect("adm_idea_modifiers");
        assert_eq!(adm.scalars.iter().find(|s| s.key == "category").unwrap().value, "ADM");
        assert!(adm.ordered_children.len() >= 3, "adm custom-idea category has several ideas");
        let aip = load(&vfs, &loc, "ai_personalities").unwrap();
        assert!(aip.objects.iter().any(|o| o.key == "ai_capitalist"));
        let ins = load(&vfs, &loc, "insults").unwrap();
        assert!(ins.objects.iter().any(|o| o.script_blocks.iter().any(|s| s.name == "trigger" && s.present)));
    }
}
