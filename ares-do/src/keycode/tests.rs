use super::{KEY_MAX, alias, list, lookup, parse, table};

#[test]
fn table_is_sorted_and_unique() {
    for pair in table::KEYCODES.windows(2) {
        assert!(
            pair[0].0 < pair[1].0,
            "table must be sorted and unique for binary search: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn table_codes_are_in_range() {
    for (name, code) in table::KEYCODES {
        assert!(*code <= KEY_MAX, "{name} = {code} is above KEY_MAX");
    }
}

#[test]
fn the_codes_from_the_field_notes_still_hold() {
    // Observed on a 49LK5900, webOS 4.4.3. If a regenerated table moves any of
    // these, the tool silently starts pressing different buttons.
    for (name, code) in [
        ("ENTER", 28),
        ("TAB", 15),
        ("KEY_BACK", 158),
        ("EXIT", 174),
        ("UP", 103),
        ("DOWN", 108),
        ("LEFT", 105),
        ("RIGHT", 106),
    ] {
        assert_eq!(parse(name).unwrap().code, code, "{name}");
    }
}

#[test]
fn every_alias_points_at_a_real_key() {
    for (from, to) in alias::ALIASES {
        assert!(lookup(to).is_some(), "alias {from} points at missing {to}");
    }
}

#[test]
fn an_alias_that_shadows_a_real_key_has_to_say_why() {
    // Shadowing is occasionally right — LG's OK button sends ENTER (28), not
    // the kernel's KEY_OK (352) — but never by accident. Anything that hides
    // a real name must explain itself in NOTES, where `ares-do keys` shows it.
    for (from, to) in alias::ALIASES {
        let Some((_, shadowed)) = lookup(from) else {
            continue;
        };
        let note = alias::NOTES.iter().find(|(name, _)| name == from);
        assert!(
            note.is_some(),
            "{from} is already a real key ({shadowed}); aliasing it to {to} hides that, \
             so it needs a NOTES entry saying so"
        );
    }
}

#[test]
fn aliases_are_sorted() {
    for pair in alias::ALIASES.windows(2) {
        assert!(pair[0].0 < pair[1].0, "keep ALIASES sorted for readability");
    }
}

#[test]
fn curated_keys_and_notes_exist() {
    for name in alias::TV_KEYS {
        assert!(
            lookup(name).is_some(),
            "curated key {name} is not in the table"
        );
    }
    for (name, _) in alias::NOTES {
        assert!(lookup(name).is_some(), "note names missing key {name}");
    }
}

#[test]
fn names_resolve_however_they_are_written() {
    let enter = parse("ENTER").unwrap();
    for spelling in [
        "ok",
        "OK",
        "Ok",
        "enter",
        "KEY_ENTER",
        "key_enter",
        "RETURN",
    ] {
        assert_eq!(parse(spelling).unwrap(), enter, "{spelling}");
    }
    assert_eq!(enter.name, Some("ENTER"));
}

#[test]
fn raw_codes_parse_in_both_bases_and_carry_no_name() {
    assert_eq!(parse("28").unwrap(), parse("0x1c").unwrap());
    assert_eq!(parse("28").unwrap().code, 28);
    assert_eq!(parse("28").unwrap().name, None);
    // KEY_RESERVED is dropped from the table but is still a sendable code.
    assert_eq!(parse("0").unwrap().code, 0);
    assert_eq!(parse("767").unwrap().code, KEY_MAX);
}

#[test]
fn out_of_range_codes_say_so_rather_than_looking_like_a_name() {
    let message = parse("768").unwrap_err();
    assert!(message.contains("out of range"), "{message}");
    assert!(!message.contains("did you mean"), "{message}");
}

#[test]
fn an_unknown_name_suggests_something_useful() {
    // "OKAY" contains "OK", so asserting on the message as a whole proves
    // nothing — check the suggestion list itself.
    let message = parse("OKAY").unwrap_err();
    assert!(message.contains("unknown key \"OKAY\""), "{message}");
    assert!(message.contains("did you mean"), "{message}");
    assert!(super::suggestions("OKAY").contains(&"OK"), "overshot name");
    assert!(message.contains("ares-do keys"), "{message}");
}

#[test]
fn suggestions_cover_the_ways_a_name_is_got_wrong() {
    use super::suggestions;
    assert!(suggestions("ENTE").contains(&"ENTER"), "unfinished");
    assert!(suggestions("OKAY").contains(&"OK"), "overshot");
    assert!(suggestions("ENTR").contains(&"ENTER"), "typo");
    assert!(suggestions("VOLUME").contains(&"VOLUMEUP"), "substring");
    assert!(suggestions("ZZZZZZZZZZ").is_empty(), "nothing close");
}

#[test]
fn nonsense_still_explains_the_way_out() {
    let message = parse("!!!").unwrap_err();
    assert!(message.contains("raw evdev code"), "{message}");
}

#[test]
fn listing_is_curated_by_default_and_complete_with_all() {
    let curated = list(false, None);
    let every = list(true, None);
    assert_eq!(curated.len(), alias::TV_KEYS.len());
    assert_eq!(every.len(), table::KEYCODES.len());
    assert!(every.len() > curated.len());
}

#[test]
fn listing_carries_aliases_and_notes() {
    let enter = list(false, Some("enter")).into_iter().next().unwrap();
    assert!(enter.aliases.contains(&"OK"), "{:?}", enter.aliases);

    let back = list(true, Some("back"))
        .into_iter()
        .find(|k| k.name == "BACK");
    assert!(back.unwrap().note.is_some(), "BACK should carry its caveat");
}

#[test]
fn listing_filters_on_aliases_too() {
    // "VOLUP" is only an alias, so this only matches if aliases are searched.
    let hits = list(true, Some("volup"));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "VOLUMEUP");
}

#[test]
fn the_lg_remote_buttons_map_where_the_firmware_says() {
    // From /usr/share/X11/xkb/keycodes/lg, minus the xkb +8 offset. These are
    // the buttons whose evdev code is not the mainline name you would guess,
    // and getting one wrong means pressing a different button on the TV.
    for (alias, code) in [
        ("REMOTE_BACK", 412),
        ("REMOTE_GUIDE", 362),
        ("REMOTE_TVGUIDE", 362),
        ("REMOTE_RECLIST", 144),
        ("REMOTE_TV_VIDEO", 241),
        ("REMOTE_INPUT_SOURCE", 241),
        ("REMOTE_VOICE", 428),
        ("REMOTE_FAVORITE", 364),
        ("REMOTE_MENU", 139),
        ("REMOTE_SETTINGS", 139),
    ] {
        assert_eq!(parse(alias).unwrap().code, code, "{alias}");
    }
}

#[test]
fn the_launcher_key_is_reachable_by_every_name_for_it() {
    for name in ["SUPER", "META", "LAUNCHER", "LEFTMETA"] {
        assert_eq!(parse(name).unwrap().code, 125, "{name}");
    }
}

#[test]
fn back_is_still_the_kernels_back_and_says_it_is_not_the_remotes() {
    // Keeping 158 under its own name matters: silently remapping it would
    // make `ares-do key BACK` send something the name does not say.
    assert_eq!(parse("KEY_BACK").unwrap().code, 158);
    let note = list(true, Some("BACK"))
        .into_iter()
        .find(|k| k.name == "BACK")
        .and_then(|k| k.note)
        .unwrap();
    assert!(note.contains("REMOTE_BACK"), "{note}");
}

#[test]
fn a_digit_by_name_is_the_digit_key_not_the_raw_code() {
    // `text "0"` must press KEY_0 (11), not evdev code 0. `key 0` still means
    // the raw code — that is the whole difference between the two lookups.
    use super::by_name;
    assert_eq!(by_name("0").unwrap().code, 11);
    assert_eq!(by_name("9").unwrap().code, 10);
    assert_eq!(parse("0").unwrap().code, 0);
}

#[test]
fn by_name_refuses_numbers() {
    use super::by_name;
    assert!(super::by_name("28").is_none());
    assert_eq!(by_name("ok").unwrap().code, 28);
}

#[test]
fn an_ambiguous_bare_name_is_refused_rather_than_guessed() {
    // The one that matters. Bare BACK used to resolve to 158, which is a real
    // key that the platform swallows, so it looked like the tool had worked.
    let message = parse("BACK").unwrap_err();
    assert!(message.contains("ambiguous"), "{message}");
    assert!(message.contains("REMOTE_BACK"), "{message}");
    assert!(message.contains("412"), "{message}");
    assert_eq!(
        parse("back").unwrap_err(),
        message,
        "case must not dodge it"
    );
}

#[test]
fn both_readings_stay_reachable_by_saying_which() {
    assert_eq!(parse("REMOTE_BACK").unwrap().code, 412);
    assert_eq!(parse("KEY_BACK").unwrap().code, 158);
    assert_eq!(parse("XF86BACK").unwrap().code, 158);
    assert_eq!(parse("xf86back").unwrap().code, 158);
    assert_eq!(parse("158").unwrap().code, 158);
}

#[test]
fn the_xf86_prefix_works_for_ordinary_keys_too() {
    assert_eq!(parse("XF86EXIT").unwrap().code, parse("EXIT").unwrap().code);
}

#[test]
fn every_colliding_name_is_declared_ambiguous() {
    // The rule this list exists for, checked rather than remembered: if a
    // REMOTE_X and a bare X disagree about the code, X must refuse.
    for (remote_name, remote_code) in super::remote::REMOTE {
        let Some(bare) = remote_name.strip_prefix("REMOTE_") else {
            continue;
        };
        let Some((_, kernel_code)) = lookup(bare) else {
            continue;
        };
        if kernel_code != remote_code {
            assert!(
                alias::AMBIGUOUS.iter().any(|(n, _)| n == &bare),
                "{bare} is {kernel_code} to the kernel and {remote_code} to the remote, \
                 so it must be in AMBIGUOUS"
            );
        }
    }
}

#[test]
fn remote_names_are_sorted_and_case_insensitive() {
    for pair in super::remote::REMOTE.windows(2) {
        assert!(pair[0].0 < pair[1].0, "{:?} then {:?}", pair[0], pair[1]);
    }
    assert_eq!(
        parse("remote_exit").unwrap().code,
        parse("REMOTE_EXIT").unwrap().code
    );
}

#[test]
fn every_remote_code_is_deliverable() {
    // sendKeyCode goes through /dev/uinput, so anything above KEY_MAX cannot
    // arrive however it is spelled. The generator drops those; check it did.
    for (name, code) in super::remote::REMOTE {
        assert!(*code <= KEY_MAX, "{name} = {code}");
    }
}

#[test]
fn a_mistyped_remote_name_suggests_a_real_one() {
    assert!(super::suggestions("REMOTE_BAC").contains(&"REMOTE_BACK"));
}
