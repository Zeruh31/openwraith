// The Actuation page (owned by the actuation page agent): the Actuation &
// Rapid Trigger tab with its presets, and the Dead band, SOCD and Key depth
// tabs.

import type { DeadBandHintKind, DepthStopReason, EditFieldView, TriggerDisabled } from "../../ipc/bindings";

export const actuation = {
  title: "Actuation & Rapid Trigger",
  description: "Choose keys on the keyboard, then set where they trigger and how Rapid Trigger behaves.",
  board: {
    title: (p: { n: number }) => `Keys on profile ${p.n}`,
    titleUnknown: "Keys",
    label: "Keys of the keyboard",
    readAt: (p: { time: string }) => `Read at ${p.time}`,
    readAgain: "Read the keys again",
    knob: "The knob has no trigger settings.",
  },
  show: {
    label: "Show on keys",
    actuation: "Actuation",
    rapidTrigger: "Rapid Trigger",
  },
  select: {
    label: "Select",
    all: "All",
    none: "None",
    invert: "Invert",
    same: "Same setting",
    sameHint: "Adds every key that has the same setting as a selected key.",
    changed: "Changed",
    changedHint: "Selects the keys that differ from the basic setting.",
    wasd: "WASD",
    arrows: "Arrows",
    rowLabel: "Select a row",
    rowPlaceholder: "Row…",
    rows: {
      function: "Top row (Esc to Del)",
      number: "Number row (` to Home)",
      top: "Q row (Tab to PgUp)",
      home: "A row (Caps Lock to PgDn)",
      bottom: "Z row (Shift to Up)",
      space: "Bottom row (Ctrl to Right)",
    },
    hint: "Click keys to select them, drag across keys to select several, or hold Shift to select a block. Alt + drag removes keys.",
  },
  /** Key cap legends by the library's key name. Short, the same in every language (the caps are printed in English). */
  legends: {
    Esc: "ESC",
    Del: "DEL",
    Knob: "KNOB",
    Backspace: "⌫",
    Home: "HOME",
    Tab: "TAB",
    PgUp: "PGUP",
    "Caps Lock": "CAPS",
    Enter: "ENTER",
    PgDn: "PGDN",
    "L-Shift": "SHIFT",
    "R-Shift": "SHIFT",
    Up: "↑",
    "L-Ctrl": "CTRL",
    "L-Win": "WIN",
    "L-Alt": "ALT",
    Space: "SPACE",
    "R-Alt": "ALT",
    Fn: "FN",
    "R-Ctrl": "CTRL",
    Left: "←",
    Down: "↓",
    Right: "→",
  },
  chip: "RT",
  values: {
    off: "–",
    unknown: "?",
    separate: (p: { press: string; release: string }) => `↓${p.press} ↑${p.release}`,
  },
  setting: {
    full: (p: { actuation: string; rt: string }) => `${p.actuation} · ${p.rt}`,
    rtOff: "RT off",
    rtSame: (p: { value: string }) => `RT ${p.value}`,
    rtSeparate: (p: { press: string; release: string }) => `RT ${p.press} / ${p.release}`,
    rtUnknown: "RT unknown",
  },
  keyInfo: {
    actuation: (p: { value: string }) => `actuation ${p.value}`,
    rtOff: "Rapid Trigger off",
    rtSame: (p: { value: string }) => `Rapid Trigger ${p.value}`,
    rtSeparate: (p: { press: string; release: string }) =>
      `Rapid Trigger ${p.press} press, ${p.release} release`,
    unknown: "settings unknown",
    draft: (p: { next: string }) => `new setting ${p.next}, not written yet`,
    changed: "differs from the basic setting",
  },
  legend: {
    label: "Legend",
    selected: "selected",
    pending: "not written yet",
    warning: "can't be changed",
    rt: "Rapid Trigger on",
    pressRelease: "↓ press, ↑ release sensitivity",
    changed: (p: { value: string }) => `differs from the basic setting (${p.value}, Rapid Trigger off)`,
  },
  inspector: {
    title: "Selected keys",
    none: "No keys selected. Click keys on the keyboard, drag across them, or use the buttons above it.",
    count: (p: { n: number }) => (p.n === 1 ? "1 key selected" : `${p.n} keys selected`),
    now: "Now:",
    actuationSame: (p: { value: string }) => `Actuation ${p.value}`,
    actuationMixed: (p: { min: string; max: string }) => `Actuation ${p.min}–${p.max} (mixed)`,
    rtGroup: (p: { what: string; n: number }) => (p.n === 1 ? `${p.what} on 1 key` : `${p.what} on ${p.n} keys`),
    leftOut: (p: { key: string }) => `${p.key} will be left out: OpenWraith can't change its settings safely.`,
    readOnly: "These values can't be changed right now.",
  },
  actuationControl: {
    title: "Actuation point",
    change: "Change the actuation point",
    label: "Actuation point",
    hint: "Lower values make the key trigger earlier.",
    range: (p: { min: string; max: string }) => `From ${p.min} to ${p.max}.`,
    unchanged: "Not changed: each key keeps its own actuation point.",
  },
  rt: {
    title: "Rapid Trigger",
    keep: "Don't change",
    keepHint: "Each key keeps its own Rapid Trigger setting.",
    off: "Off",
    offHint: "The key turns on and off at the actuation point.",
    same: "On",
    sameHint: "One sensitivity for pressing and releasing.",
    separate: "On, separate press and release",
    separateHint: "Different sensitivities for pressing and releasing.",
    sensitivity: "Sensitivity",
    press: "Press sensitivity",
    release: "Release sensitivity",
    sensitivityHint: "Smaller values react to shorter movements.",
    explainTitle: "What is Rapid Trigger?",
    explain:
      "With Rapid Trigger, a key turns off as soon as it rises by the release sensitivity and turns on again as soon as it goes down by the press sensitivity, wherever it is in its travel. Without it, the key turns on and off only at the actuation point.",
  },
  apply: {
    idle: "No unwritten changes. Select keys, then choose new values.",
    setValues: "Set values ↓",
    notWritten: "Not written yet",
    willChange: (p: { n: number }) => (p.n === 1 ? "1 key will change" : `${p.n} keys will change`),
    already: (p: { n: number }) =>
      p.n === 1 ? "1 already has these settings" : `${p.n} already have these settings`,
    leftOut: (p: { n: number }) => `${p.n} left out (⚠)`,
    reasons: {
      selectKeys: "Select at least one key.",
      noEditable: "None of the selected keys can be changed.",
      nothing: "The selected keys already have these settings.",
    },
  },
  disabled: {
    distance_unit:
      "This keyboard reports a different distance unit. OpenWraith can show its values but can't change them safely.",
    limits_unknown: "The keyboard's trigger limits aren't known, so OpenWraith can't change these settings safely.",
    unsupported_model: "OpenWraith can't change trigger settings on this keyboard model.",
  } satisfies Record<TriggerDisabled, string>,
  stale: "The values shown may be out of date. Read the keyboard again before changing anything.",
  empty: {
    title: "No key settings yet",
    body: "Connect the keyboard. Its actuation and Rapid Trigger settings appear here once it has been read.",
    reading: "Reading the keyboard…",
  },
  draftDiscarded: (p: { n: number }) =>
    `Your unwritten actuation changes were discarded: the keyboard is on profile ${p.n} now.`,
  draftDiscardedReconnect: "Your unwritten actuation changes were discarded because the keyboard was connected again.",
  review: {
    title: (p: { n: number; profile: number }) =>
      p.n === 1 ? `Change 1 key on profile ${p.profile}?` : `Change ${p.n} keys on profile ${p.profile}?`,
    titleNothing: "Nothing to change",
    intro: (p: { profile: number }) =>
      `OpenWraith read these keys again just now. The new settings go to profile ${p.profile}, one key at a time.`,
    key: "Key",
    now: "Now",
    next: "New",
    unchanged: (p: { n: number }) => `Already have these settings (${p.n})`,
    leftOut: (p: { n: number }) =>
      p.n === 1
        ? "1 selected key is left out because OpenWraith can't change it safely."
        : `${p.n} selected keys are left out because OpenWraith can't change them safely.`,
    packets: (p: { n: number }) => (p.n === 1 ? "1 write packet, one per key." : `${p.n} write packets, one per key.`),
    nothing: "Nothing to change: the selected keys already have these settings.",
    preset: (p: { name: string }) => `Preset: ${p.name}`,
  },
  /** Words shared by the dead band and SOCD tabs and the presets. */
  edit: {
    /** Why an edit was refused (`ErrorDetail::Edit`); nothing was written. */
    problems: {
      no_changes: "Nothing would change.",
      too_many_keys: (p: { max: number }) => `There are too many: at most ${p.max} are allowed.`,
      duplicate_key: (p: { key: string }) => `${p.key} is listed twice.`,
      not_a_key: (p: { index: number }) => `Key number ${p.index} isn't a key of this keyboard.`,
      knob: "The knob can't be changed here.",
      invalid_color: "That isn't a valid colour.",
      out_of_range: (p: { field: string; value: string; min: string; max: string }) =>
        `${p.field} ${p.value} is outside the allowed range (${p.min} to ${p.max}). OpenWraith doesn't round it to fit.`,
      no_hid_usage: (p: { key: string }) => `${p.key} can't be used in a SOCD pair.`,
      socd_same_key: (p: { key: string }) => `${p.key} can't be paired with itself.`,
      socd_duplicate_key: (p: { key: string }) => `${p.key} is in more than one pair; a key can be in one pair only.`,
      unknown_preset: "That preset no longer exists.",
      preset_name: "Give the preset a name of 1 to 40 characters.",
      too_many_presets: (p: { max: number }) => `You can keep at most ${p.max} presets.`,
      unsupported_current_table: (p: { table: string }) =>
        `Undo couldn't bring back what ${p.table} holds now, so OpenWraith didn't plan the change.`,
      not_available: "This isn't available for the connected keyboard.",
      other: "The change was refused. The technical details say why.",
    },
    fields: {
      top: "Top dead zone",
      bottom: "Bottom dead zone",
      socd_mode: "SOCD mode",
      actuation: "Actuation point",
      sensitivity: "Sensitivity",
      press: "Press sensitivity",
      release: "Release sensitivity",
    } satisfies Record<EditFieldView, string>,
    warnings: {
      not_shown_until_custom_mode: "These colours show only while the light effect is Custom.",
      socd_power_all_profiles:
        "The SOCD switch is one setting of the whole keyboard: this changes it on every profile.",
      socd_hotkey: "Fn + Right Shift on the keyboard also turns SOCD on and off.",
      unverified: (p: { what: string }) => `Not yet verified on a W75: ${p.what}.`,
      not_undoable: (p: { table: string }) => `Undo can't bring back what ${p.table} holds now.`,
      other: "There is a note about this plan in the technical details.",
    },
    persistence: {
      check: (p: { m: number }) =>
        `Check that the keyboard kept it (switches to profile ${p.m} and back; the lighting changes for a moment)`,
      hint: "Optional: the keyboard is known to keep this setting. The check takes a few seconds.",
      forced: "OpenWraith checks this change by switching profiles and back.",
    },
    packets: (p: { n: number }) => (p.n === 1 ? "1 packet" : `${p.n} packets`),
    packetsTotal: (p: { n: number }) => (p.n === 1 ? "In total: 1 write packet" : `In total: ${p.n} write packets`),
    depthRunning: "The key-depth view is running. Stop it on the Key depth tab first.",
  },
  presets: {
    open: "Presets…",
    title: "Trigger presets",
    intro:
      "A preset sets the actuation point and Rapid Trigger. Choose one and the keys it goes to; OpenWraith shows the plan before anything is written.",
    loading: "Loading the presets…",
    builtinTitle: "Built-in presets",
    userTitle: "Your presets (this PC)",
    noUser: "You have no presets of your own yet. You can save the current values below.",
    /** The library's built-in presets, by id. */
    builtin: {
      typing: "Typing",
      balanced: "Balanced",
      fast: "Fast",
      competitive: "Competitive",
      factory: "Factory setting",
    },
    builtinHints: {
      typing: "Deep actuation, no Rapid Trigger: fewer accidental presses.",
      balanced: "A middle actuation point with Rapid Trigger.",
      fast: "Early actuation with a sensitive Rapid Trigger.",
      competitive: "Very early actuation with the most sensitive Rapid Trigger here.",
      factory: "The values of a new profile, Rapid Trigger off.",
    },
    userFallback: "Your preset",
    targetTitle: "Apply to",
    targets: {
      all: (p: { n: number }) => `All keys (${p.n})`,
      wasd: "WASD",
      selection: (p: { n: number }) => `Selected keys (${p.n})`,
    },
    reviewNote: "Keys whose settings OpenWraith can't change safely are left out.",
    reasons: {
      choose: "Choose a preset.",
      noKeys: "None of these keys can be changed.",
    },
    refused: (p: { why: string }) => `Can't be used with this keyboard: ${p.why}`,
    saved: (p: { name: string }) => `Preset “${p.name}” saved`,
    deleted: (p: { name: string }) => `Preset “${p.name}” deleted`,
    delete: (p: { name: string }) => `Delete the preset ${p.name}`,
    save: {
      title: "Save as a preset",
      nothing:
        "To save a preset, set both the actuation point and Rapid Trigger in the editor, or select keys that share one setting.",
      what: (p: { values: string }) => `Saves ${p.values}.`,
      name: "Name",
      button: "Save",
      nameNeeded: "Give the preset a name.",
      full: (p: { max: number }) => `You already have ${p.max} presets, the most there can be. Delete one first.`,
      refused: "The preset wasn't saved",
    },
  },
  deadband: {
    title: "Dead band",
    description:
      "Choose keys, then set how much of their travel they ignore at the top and at the bottom. Each profile has its own dead band.",
    knob: "The knob has no dead band.",
    changedHint: "Selects the keys that have a dead zone.",
    show: {
      top: "Top",
      bottom: "Bottom",
    },
    legend: {
      zoned: "has a dead zone",
      showTop: "Values: top dead zone in mm",
      showBottom: "Values: bottom dead zone in mm",
    },
    keyInfo: {
      now: (p: { top: string; bottom: string }) => `top dead zone ${p.top}, bottom dead zone ${p.bottom}`,
      draft: (p: { top: string; bottom: string }) => `new: top ${p.top}, bottom ${p.bottom}, not written yet`,
    },
    inspector: {
      title: "Selected keys",
      none: "No keys selected. Click keys on the keyboard, drag across them, or use the buttons above it.",
    },
    nowSame: (p: { label: string; value: string }) => `${p.label} ${p.value}`,
    nowMixed: (p: { label: string; min: string; max: string }) => `${p.label} ${p.min}–${p.max} (mixed)`,
    range: (p: { min: string; max: string }) => `From ${p.min} to ${p.max}.`,
    top: {
      title: "Top dead zone",
      change: "Change the top dead zone",
      label: "Top dead zone",
      hint: "Travel from the key at rest that the keyboard ignores.",
      unchanged: "Not changed: each key keeps its own top dead zone.",
    },
    bottom: {
      title: "Bottom dead zone",
      change: "Change the bottom dead zone",
      label: "Bottom dead zone",
      hint: "Travel just before the key bottoms out that the keyboard ignores.",
      unchanged: "Not changed: each key keeps its own bottom dead zone.",
    },
    gauge: {
      title: (p: { key: string }) => `On the travel of ${p.key}`,
      label: (p: { key: string }) => `${p.key}: key travel with dead zones`,
    },
    hints: {
      top_reaches_actuation: (p: { key: string; actuation: string }) =>
        `${p.key}: the top dead zone reaches its actuation point (${p.actuation}), so the key may not trigger as expected.`,
      bottom_reaches_actuation: (p: { key: string; actuation: string }) =>
        `${p.key}: the bottom dead zone reaches up to its actuation point (${p.actuation}), so the key may not trigger as expected.`,
    } satisfies Record<DeadBandHintKind, unknown>,
    explainTitle: "What is the dead band?",
    explain:
      "A dead zone is a part of a key's travel that the keyboard ignores. The top dead zone starts at the key at rest; the bottom dead zone ends where the key bottoms out. The names come from the Wraith website, which doesn't say more about how the keyboard uses them. A dead zone that reaches the actuation point may stop the key from triggering as expected.",
    setting: (p: { top: string; bottom: string }) => `Top ${p.top} · bottom ${p.bottom}`,
    settingTop: (p: { value: string }) => `Top ${p.value}`,
    settingBottom: (p: { value: string }) => `bottom ${p.value}`,
    apply: {
      idle: "No unwritten changes. Select keys, then change their dead zones.",
      nothing: "The selected keys already have these dead zones.",
    },
    disabled: {
      not_read: "The dead band wasn't read from this keyboard, so OpenWraith can't change it.",
      distance_unit:
        "This keyboard reports a different distance unit. OpenWraith can show the dead band but can't change it safely.",
      unreadable: "OpenWraith couldn't read the dead band of this keyboard, so it can't change it safely.",
      unsupported_model: "OpenWraith can't change the dead band on this keyboard model.",
    },
    empty: {
      title: "No dead band yet",
      body: "Connect the keyboard. Its dead band appears here once it has been read.",
      failed: "OpenWraith couldn't load the dead band. Read the keyboard again.",
    },
    discarded: (p: { n: number }) =>
      `Your unwritten dead band changes were discarded: the keyboard is on profile ${p.n} now.`,
    discardedReconnect: "Your unwritten dead band changes were discarded because the keyboard was connected again.",
    review: {
      title: (p: { n: number; profile: number }) =>
        p.n === 1
          ? `Change the dead band of 1 key on profile ${p.profile}?`
          : `Change the dead band of ${p.n} keys on profile ${p.profile}?`,
      titleNothing: "Nothing to change",
      intro: (p: { profile: number }) =>
        `OpenWraith planned this from the last read. The new dead band goes to profile ${p.profile}.`,
      nothing: "Nothing to change: the selected keys already have these dead zones.",
    },
  },
  socd: {
    title: "SOCD",
    description:
      "When two opposite keys, such as A and D, are held at once, SOCD decides which one counts. Turn it on and choose the pairs.",
    readAgain: "Read the keyboard again",
    power: {
      title: "SOCD switch",
      label: "SOCD on",
      scope:
        "This switch is one setting of the whole keyboard: it turns SOCD on or off on every profile. The pairs below belong to the active profile.",
      hotkey: "Fn + Right Shift on the keyboard also turns it on and off.",
      pending: (p: { on: boolean }) =>
        p.on
          ? "Not written yet: SOCD will be turned on for every profile."
          : "Not written yet: SOCD will be turned off for every profile.",
      toggled: (p: { on: boolean }) =>
        p.on
          ? "SOCD was turned on with Fn + Right Shift. The switch shows it now."
          : "SOCD was turned off with Fn + Right Shift. The switch shows it now.",
      on: "on",
      off: "off",
    },
    games: {
      title: "Check the game's rules",
      body: "Some games forbid SOCD-style input. Check a game's rules before you use SOCD in it.",
    },
    pairs: {
      title: (p: { n: number }) => `Pairs on profile ${p.n}`,
      intro: "Each pair is two keys. The mode decides what happens while both are held.",
      count: (p: { n: number; max: number }) => `${p.n} of ${p.max} pairs`,
      none: "No pairs yet. Add one, or use a quick pair below.",
      notShown: "The keyboard's pairs aren't in a form OpenWraith can show as keys.",
      add: "Add a pair",
      addLabel: "Add pairs",
      quick: {
        ad: "A + D",
        ws: "W + S",
        arrows: "← + →",
      },
      quickUsed: "One of these keys is already in a pair, or there's no room for another pair.",
      pairN: (p: { n: number }) => `Pair ${p.n}`,
      key1: "Key 1",
      key2: "Key 2",
      mode: "While both are held",
      modeOption: (p: { n: number; name: string }) => `${p.n} · ${p.name}`,
      remove: "Remove",
      usedIn: (p: { n: number }) => `in pair ${p.n}`,
      otherKey: "the pair's other key",
    },
    modesTitle: "What do the modes do?",
    /** The vendor site's modes 1-4. */
    modes: {
      m1: "Last input wins",
      m2: "Key 1 wins",
      m3: "Key 2 wins",
      m4: "Later key only interrupts the first",
    },
    modeHints: {
      m1: "While both keys are held, the one pressed last counts.",
      m2: "While both keys are held, key 1 counts.",
      m3: "While both keys are held, key 2 counts.",
      m4: "The key pressed later only interrupts the first one.",
    },
    modeUnknown: (p: { mode: number }) => `Mode ${p.mode}`,
    usage: (p: { hex: string }) => `HID usage 0x${p.hex}`,
    pairKeys: (p: { key1: string; key2: string }) => `${p.key1} + ${p.key2}`,
    problems: {
      incomplete: "Choose both keys.",
      sameKey: (p: { key: string }) => `${p.key} can't be paired with itself.`,
      duplicate: (p: { key: string; pair: number }) =>
        `${p.key} is already in pair ${p.pair}; a key can be in one pair only.`,
      mode: "Choose a mode from 1 to 4.",
    },
    replace: {
      title: "These pairs are kept as read",
      body: "The keyboard holds pairs in a form the Wraith website doesn't write. OpenWraith leaves them as they are unless you replace them.",
      start: "Replace them…",
      confirm: "Replace the pairs although Undo can't bring the current ones back",
      reviewAgain: "Review again",
      needed: "Tick the box to replace them, then review again.",
    },
    apply: {
      idle: "No unwritten changes. Turn SOCD on or off, or change the pairs.",
      nothing: "Nothing has changed yet.",
      tooMany: (p: { max: number }) => `At most ${p.max} pairs.`,
      fixPairs: "Fix the pairs marked above first.",
      powerNeedsPairs:
        "This profile's key pairs are in a form OpenWraith can't keep, so SOCD can be switched on or off here only together with replacing them (“Replace them…”). Or switch it from a profile whose pairs can be written, or with Fn + Right Shift.",
      confirmReplace: "Confirm that the pairs may be replaced.",
      powerOn: "SOCD on (every profile)",
      powerOff: "SOCD off (every profile)",
      pairs: (p: { n: number }) => (p.n === 1 ? "1 pair" : `${p.n} pairs`),
    },
    disabled: {
      not_read: "SOCD wasn't read from this keyboard, so OpenWraith can't change it.",
      unreadable: "OpenWraith couldn't read SOCD from this keyboard, so it can't change it safely.",
      unsupported_model: "OpenWraith can't change SOCD on this keyboard model.",
    },
    empty: {
      title: "No SOCD settings yet",
      body: "Connect the keyboard. Its SOCD settings appear here once it has been read.",
      failed: "OpenWraith couldn't load the SOCD settings. Read the keyboard again.",
    },
    discarded: (p: { n: number }) =>
      `Your unwritten SOCD changes were discarded: the keyboard is on profile ${p.n} now.`,
    discardedReconnect: "Your unwritten SOCD changes were discarded because the keyboard was connected again.",
    review: {
      title: "Change SOCD?",
      titleNothing: "Nothing to change",
      nothing: "Nothing to change: the keyboard already has these SOCD settings.",
      intro: (p: { profile: number }) =>
        `OpenWraith planned this from the last read. The switch is one setting of the whole keyboard; the pairs go to profile ${p.profile}.`,
      power: (p: { from: string; to: string }) => `SOCD: ${p.from} → ${p.to}`,
      powerSame: (p: { state: string }) => `SOCD stays ${p.state}.`,
      pairsAfter: (p: { n: number; profile: number }) => `Pairs on profile ${p.profile} after writing (${p.n})`,
      pairsBefore: (p: { n: number }) => `Pairs now (${p.n})`,
      pairsSame: (p: { n: number }) => (p.n === 1 ? "The pair doesn't change." : `The pairs don't change (${p.n}).`),
      noPairs: "No pairs.",
      pair: "Pair",
      keys: "Keys",
      mode: "While both are held",
    },
  },
  depth: {
    title: "Key depth",
    description: "See how far each key is pressed, live. Nothing is written to the keyboard's settings.",
    note: "Slow presses update about 14 times a second. This is a visualiser, not analog input.",
    controls: {
      title: "Live view",
      start: "Start",
      resume: "Resume",
      stop: "Stop",
      stopping: "Stopping…",
      idle: "Not running.",
      opening: "Starting…",
      live: "Live: press keys to see how far they go.",
      blocksWrites: "While the view runs, nothing else can be written to the keyboard. Leaving this tab stops it.",
      keysHeld: "While the view runs, the keys you press don't scroll the page or press buttons here. Esc stops the view.",
      otherView: "A key-depth view started earlier is still running.",
    },
    options: {
      holdPeak: "Hold the deepest point",
      clearPeaks: "Clear",
      showTrigger: "Show where keys trigger (estimated)",
      announce: "Announce the depth",
      announceHint: "For screen readers: says the depth of the followed key, at most four times a second.",
      estimatedNote:
        "Estimated from each key's actuation and Rapid Trigger settings as last read. The keyboard reports slow presses about 14 times a second, so a short press can be missed.",
    },
    stopped: {
      requested: "Stopped.",
      window_hidden: "Stopped because the window was hidden.",
      page_left: "Stopped because you left the tab.",
      app_exit: "Stopped because OpenWraith is closing.",
      disconnected: "Stopped: the keyboard was disconnected.",
      keyboard_reset: "Stopped: the keyboard reported a reset.",
      other_program: "Stopped: another program is using the keyboard.",
      profile_changed: "Stopped: the keyboard switched profiles.",
      channel_closed: "Stopped: the window lost its link to the view (for example after a reload).",
      no_movement: "Paused: no key moved for 5 minutes.",
      time_limit: "Stopped after 60 minutes. Start it again to go on.",
    } satisfies Record<DepthStopReason, string>,
    close: {
      notAcked: "The keyboard didn't confirm that it stopped sending key depths. If “Stop it” is offered, use it.",
      notSent: "OpenWraith couldn't tell the keyboard to stop sending key depths. If “Stop it” is offered, use it.",
    },
    technical: (p: { reports: number; dropped: number; seconds: string; close: string }) =>
      `Reports: ${p.reports} · dropped: ${p.dropped} · open for ${p.seconds} · close: ${p.close}`,
    stray: {
      title: "The key-depth view may still be on in the keyboard",
      body: "A key-depth view may not have been stopped. Typing keeps working, but OpenWraith may refuse to write settings until the keyboard is told to stop.",
      stop: "Stop it",
      closed: "The keyboard's key-depth view is stopped.",
      whileRunning: "Stop the live view first.",
    },
    board: {
      title: "Keys",
      label: "Keys of the keyboard: depth",
      knob: "The knob has no depth.",
      hint: "Select a key to follow it; otherwise the gauge follows the key that moved last.",
    },
    table: {
      caption: "Key depth now",
      key: "Key",
      now: "Now",
      peak: "Deepest",
      state: "Triggered (estimated)",
    },
    keyInfo: {
      rest: "at rest",
      now: (p: { value: string }) => `pressed ${p.value}`,
      peak: (p: { value: string }) => `deepest ${p.value}`,
      on: "triggered (estimated)",
      off: "not triggered (estimated)",
    },
    peakChip: "max",
    onBadge: "ON",
    announce: {
      text: (p: { key: string; value: string }) => `${p.key} ${p.value}`,
    },
    detail: {
      none: "No key yet",
      title: (p: { key: string }) => `Key ${p.key}`,
      hint: "Press a key to see its depth here.",
      peak: (p: { value: string }) => `Deepest: ${p.value}`,
      estimatedOn: "Triggered (estimated)",
      estimatedOff: "Not triggered (estimated)",
      gauge: (p: { key: string }) => `${p.key}: key travel`,
      chart: "The last 5 seconds",
    },
  },
} as const;
