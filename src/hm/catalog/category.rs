//! What a sound is, going by what it is called.
//!
//! The rules below are read in order and the first one that matches wins, so
//! the specific ones come first: a bird calling in `amb/animals/birds/generic/
//! chatter` is an animal, not a soldier, even though `chatter` is what a
//! soldier's radio does.
//!
//! Each rule also names a finer bucket — birds, doors, glass — which the
//! `tree+` view hangs its third level off. Nothing is invented here: the
//! bucket is the rule that matched, so a sound never claims to be something
//! more specific than the word in its own name.

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Category {
    Weapons,
    Voice,
    Music,
    Ambient,
    Animals,
    Weather,
    Water,
    Fire,
    Foley,
    Footsteps,
    Vehicles,
    Explosions,
    Impacts,
    Destruction,
    Doors,
    Machines,
    Alarms,
    Gore,
    Traps,
    Ui,
    Killstreaks,
    Misc,
}

pub const ALL: &[Category] = &[
    Category::Weapons,
    Category::Voice,
    Category::Music,
    Category::Ambient,
    Category::Animals,
    Category::Weather,
    Category::Water,
    Category::Fire,
    Category::Foley,
    Category::Footsteps,
    Category::Vehicles,
    Category::Explosions,
    Category::Impacts,
    Category::Destruction,
    Category::Doors,
    Category::Machines,
    Category::Alarms,
    Category::Gore,
    Category::Traps,
    Category::Ui,
    Category::Killstreaks,
    Category::Misc,
];

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Weapons => "weapons",
            Category::Voice => "voice",
            Category::Music => "music",
            Category::Ambient => "ambient",
            Category::Animals => "animals",
            Category::Weather => "weather",
            Category::Water => "water",
            Category::Fire => "fire",
            Category::Foley => "foley",
            Category::Footsteps => "footsteps",
            Category::Vehicles => "vehicles",
            Category::Explosions => "explosions",
            Category::Impacts => "impacts",
            Category::Destruction => "destruction",
            Category::Doors => "doors",
            Category::Machines => "machines",
            Category::Alarms => "alarms",
            Category::Gore => "gore",
            Category::Traps => "traps",
            Category::Ui => "ui",
            Category::Killstreaks => "killstreaks",
            Category::Misc => "misc",
        }
    }
}

pub struct Rule {
    pub matches: &'static [&'static str],
    pub category: Category,
    /// The finer bucket this rule stands for.
    pub sub: &'static str,
}

/// Order matters.
///
/// Animals come before voice, because `chatter` is a bird as often as it is a
/// soldier and the folder says which. Voice comes before weapons, because a
/// line of battlechatter is called `us_pol_inform_reloading_generic_01`: it is
/// a soldier saying he is reloading, not a rifle.
pub const RULES: &[Rule] = &[
    Rule {
        matches: &["bird", "seagull", "crow", "raven_", "pigeon", "gull_", "owl_"],
        category: Category::Animals,
        sub: "birds",
    },
    Rule {
        matches: &["dog_", "dogs", "k9_", "canine", "bark_dog", "growl"],
        category: Category::Animals,
        sub: "dogs",
    },
    Rule {
        matches: &["insect", "cicada", "cricket", "fly_", "flies", "mosquito", "bee_"],
        category: Category::Animals,
        sub: "insects",
    },
    Rule {
        matches: &["frog", "toad_"],
        category: Category::Animals,
        sub: "frogs",
    },
    Rule {
        matches: &["monkey", "chimp"],
        category: Category::Animals,
        sub: "monkeys",
    },
    Rule {
        matches: &["wolf", "wolves", "howl"],
        category: Category::Animals,
        sub: "wolves",
    },
    Rule {
        matches: &["rat_", "rats_", "bat_", "bats_", "rodent"],
        category: Category::Animals,
        sub: "vermin",
    },
    Rule {
        matches: &["horse", "cattle", "goat", "chicken", "cow_", "pig_"],
        category: Category::Animals,
        sub: "livestock",
    },
    Rule {
        matches: &["animal", "creature", "zmb_animal"],
        category: Category::Animals,
        sub: "other",
    },
    Rule {
        matches: &["battlechatter", "bcs_", "chatter_plr", "bark_"],
        category: Category::Voice,
        sub: "battlechatter",
    },
    Rule {
        matches: &["announcer", "commander_", "killcam_vo"],
        category: Category::Voice,
        sub: "announcer",
    },
    Rule {
        matches: &["radio_", "comms_", "intercom"],
        category: Category::Voice,
        sub: "radio",
    },
    Rule {
        matches: &["walla", "crowd_vox", "crowd_voice"],
        category: Category::Voice,
        sub: "crowds",
    },
    Rule {
        matches: &["breath", "gasp", "grunt", "effort", "pain_", "scream", "cough", "vomit"],
        category: Category::Voice,
        sub: "efforts",
    },
    Rule {
        matches: &[
            "voiceover",
            "voice_over",
            "voices",
            "vox_",
            "_vox",
            "dlg_",
            "dialog",
            "vo_",
            "_vo_",
            "chatter",
            "chr_",
            "conv_",
            "mission_dialog",
        ],
        category: Category::Voice,
        sub: "voiceover",
    },
    Rule {
        matches: &["mus_", "music", "mx_", "stinger"],
        category: Category::Music,
        sub: "score",
    },
    Rule {
        matches: &["reload", "mech_", "bolt_", "magazine", "charging_handle"],
        category: Category::Weapons,
        sub: "handling",
    },
    Rule {
        matches: &["fire_plr", "fire_npc", "weap_fire", "gunfire", "shot_", "gunshot", "burst_"],
        category: Category::Weapons,
        sub: "fire",
    },
    Rule {
        matches: &["wpn_", "weap", "gun_", "ads_", "melee", "knife", "suppress"],
        category: Category::Weapons,
        sub: "other",
    },
    Rule {
        matches: &["thunder", "lightning", "storm_", "rainfall", "snow_", "blizzard"],
        category: Category::Weather,
        sub: "weather",
    },
    Rule {
        matches: &[
            "water", "splash", "wave_", "ocean", "river_", "swim", "underwater", "drip",
            "waterfall", "sewer_",
        ],
        category: Category::Water,
        sub: "water",
    },
    Rule {
        matches: &["napalm", "flamethrower", "flame", "fire_", "_fire", "burn", "ember", "torch_"],
        category: Category::Fire,
        sub: "fire",
    },
    Rule {
        matches: &["door", "gate_", "hatch", "latch", "drawer", "curtain", "shutter"],
        category: Category::Doors,
        sub: "doors",
    },
    Rule {
        matches: &["alarm", "siren", "klaxon", "buzzer", "air_raid"],
        category: Category::Alarms,
        sub: "alarms",
    },
    Rule {
        matches: &["glass_", "_glass", "shatter", "pane_"],
        category: Category::Destruction,
        sub: "glass",
    },
    Rule {
        matches: &["debris", "collapse", "rubble", "crumble", "destruct", "splinter", "dst_"],
        category: Category::Destruction,
        sub: "collapse",
    },
    Rule {
        matches: &["gore", "flesh", "blood", "bone_", "squish", "dismember"],
        category: Category::Gore,
        sub: "gore",
    },
    Rule {
        matches: &["trap_", "pressure_plate", "pap_", "flogger", "spikes_", "teleporter"],
        category: Category::Traps,
        sub: "traps",
    },
    Rule {
        matches: &[
            "generator",
            "elevator",
            "minecart",
            "mine_cart",
            "conveyor",
            "turbine",
            "powerstation",
            "poweron",
            "machine_",
            "motor_",
            "pump_",
        ],
        category: Category::Machines,
        sub: "machines",
    },
    Rule {
        matches: &["amb_", "emt_", "room_", "nature", "ambient"],
        category: Category::Ambient,
        sub: "ambience",
    },
    Rule {
        matches: &["foley", "cloth", "gear_", "mvmt_", "handling"],
        category: Category::Foley,
        sub: "movement",
    },
    Rule {
        matches: &["step_", "fs_", "foot"],
        category: Category::Footsteps,
        sub: "footsteps",
    },
    Rule {
        matches: &["heli", "rotor"],
        category: Category::Vehicles,
        sub: "helicopters",
    },
    Rule {
        matches: &["plane", "jet_", "airplane", "ac130"],
        category: Category::Vehicles,
        sub: "aircraft",
    },
    Rule {
        matches: &["exp_", "expl", "grenade", "ied_", "blast", "c4_", "rpg_"],
        category: Category::Explosions,
        sub: "explosions",
    },
    Rule {
        matches: &["tank_", "apc_", "truck", "car_", "automobile", "veh_", "vehicle", "engine"],
        category: Category::Vehicles,
        sub: "ground",
    },
    Rule {
        matches: &["ric_", "whizby", "bullet"],
        category: Category::Impacts,
        sub: "bullets",
    },
    Rule {
        matches: &["imp_", "impact", "phys_", "thud_", "bounce"],
        category: Category::Impacts,
        sub: "impacts",
    },
    Rule {
        matches: &["uin_", "ui_", "menu", "hud_", "hit_marker", "interface"],
        category: Category::Ui,
        sub: "interface",
    },
    Rule {
        matches: &["ks_", "streak", "killstreak", "uav", "care_package"],
        category: Category::Killstreaks,
        sub: "killstreaks",
    },
];

/// Words that only count as whole words.
///
/// `wind` is inside `window`, `rain` is inside `terrain` and `drain`. Asking
/// for the whole word costs one split and stops a broken window becoming
/// weather.
pub const WORDS: &[(&[&str], Category, &str)] = &[
    (&["wind", "winds", "windgust"], Category::Weather, "wind"),
    (&["rain", "raining"], Category::Weather, "weather"),
    (&["dog", "dogs", "k9"], Category::Animals, "dogs"),
    (&["bird", "birds"], Category::Animals, "birds"),
    (&["bat", "bats", "rat", "rats"], Category::Animals, "vermin"),
    (&["ape", "apes", "monkey", "monkeys"], Category::Animals, "monkeys"),
    (&["fly", "flies", "insect", "insects"], Category::Animals, "insects"),
    (&["frog", "frogs"], Category::Animals, "frogs"),
    (&["wolf", "wolves"], Category::Animals, "wolves"),
    (&["horse", "horses"], Category::Animals, "livestock"),
];

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
}

/// The rule that claims this name, if any.
fn rule_of(name: &str) -> Option<&'static Rule> {
    let lower = name.to_ascii_lowercase();
    RULES
        .iter()
        .find(|rule| rule.matches.iter().any(|m| lower.contains(m)))
}

/// What the whole-word list makes of a name.
fn word_rule(name: &str) -> Option<(Category, &'static str)> {
    let lower = name.to_ascii_lowercase();
    let parts: Vec<&str> = words(&lower).collect();
    WORDS
        .iter()
        .find(|(wanted, _, _)| parts.iter().any(|part| wanted.contains(part)))
        .map(|(_, category, sub)| (*category, *sub))
}

pub fn of(name: &str) -> Category {
    // The whole words go first, because a folder is a whole word: a bark in
    // `aml/dog/pain/pain_00` is a dog in pain, not a soldier in pain, and only
    // the folder says so.
    word_rule(name)
        .map(|(category, _)| category)
        .or_else(|| rule_of(name).map(|rule| rule.category))
        .unwrap_or(Category::Misc)
}

/// The finer bucket a name falls in, for the views that show one.
pub fn sub_of(name: &str) -> &'static str {
    word_rule(name)
        .map(|(_, sub)| sub)
        .or_else(|| rule_of(name).map(|rule| rule.sub))
        .unwrap_or("unsorted")
}

pub fn parse(label: &str) -> Category {
    ALL.iter()
        .copied()
        .find(|category| category.label() == label)
        .unwrap_or(Category::Misc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bird_is_not_a_soldier() {
        assert_eq!(of("sound/amb/animals/birds/generic/chatter/chatter_00.wav"), Category::Animals);
        assert_eq!(sub_of("sound/amb/animals/birds/generic/chatter/chatter_00.wav"), "birds");
    }

    #[test]
    fn battlechatter_is_still_voice() {
        assert_eq!(
            of("battlechatter/us_pol_inform_reloading_generic_01"),
            Category::Voice
        );
        assert_eq!(of("vox/scripted/frontend/vox_fro1_s01_011a_weav.wav"), Category::Voice);
    }

    #[test]
    fn the_finer_buckets_are_the_rules_that_matched() {
        assert_eq!(sub_of("explosions/glass_window_break_med1.wav"), "glass");
        assert_eq!(sub_of("doors/door_wd_kick.wav"), "doors");
        assert_eq!(sub_of("sound/amb/alarms/siren/siren_00.wav"), "alarms");
        assert_eq!(sub_of("sound/raven/evt/zmb_temple/mine_cart/evt_minecart_l.wav"), "machines");
        assert_eq!(sub_of("sound/amb/animals/monkey/monkey_00.wav"), "monkeys");
    }

    #[test]
    fn a_folder_names_the_animal() {
        assert_eq!(of("sound/aml/dog/pain/pain_00.wav"), Category::Animals);
        assert_eq!(sub_of("sound/aml/dog/pain/pain_00.wav"), "dogs");
        assert_eq!(of("sound/evt/int_escape/hudson_stinger.wav"), Category::Music);
    }

    #[test]
    fn a_car_going_up_is_an_explosion() {
        assert_eq!(of("explosions/exp_car_close01.wav"), Category::Explosions);
        assert_eq!(of("shg/fire/fire_loops/shg_fire_lp_crackle_lrg.wav"), Category::Fire);
    }

    #[test]
    fn a_window_is_not_the_weather() {
        assert_eq!(of("explosions/glass_window_break_med1.wav"), Category::Destruction);
        assert_eq!(sub_of("amb/wind/wind_gust_01.wav"), "wind");
        assert_eq!(of("amb/weather/rain_light_loop.wav"), Category::Weather);
        assert_eq!(of("vehicles/terrain_rumble.wav"), Category::Vehicles);
    }

    #[test]
    fn a_name_with_nothing_in_it_is_misc() {
        assert_eq!(of("_a0ad9410a4d8bd37"), Category::Misc);
        assert_eq!(sub_of("_a0ad9410a4d8bd37"), "unsorted");
    }
}
