# Contributing to PIAF

Thanks for your interest in contributing. This document covers the basics.

## Getting started

Relevant docs for contributors:

- [`doc/setup.md`](doc/setup.md) — build, test, and fuzzing commands
- [`doc/testing.md`](doc/testing.md) — testing strategy, fixture corpus, and CI expectations
- [`doc/architecture.md`](doc/architecture.md) — pipeline structure, scope, and design principles
- [`doc/model.md`](doc/model.md) — data types, field conventions, and the fixed-capacity pattern
- [`doc/roadmap.md`](doc/roadmap.md) — planned features and known gaps

## Issues and pull requests

**Open an issue first** if you're unsure whether something is a bug or if you want to
discuss a change before implementing it. For small, self-contained fixes a PR on its own
is fine.

- Bug reports: include the EDID bytes (as a hex dump or attached binary) if you have them.
  To add them to the fixture corpus, see [Contributing EDID fixtures](#contributing-edid-fixtures).
- Feature requests: a brief description of what you need and why is enough to start a
  conversation.
- PRs: keep them focused. One logical change per PR makes review faster and keeps history
  readable.

## Contributing EDID fixtures

Real captures are worth most when they come with what is actually true about the display:
that is how the tests check that piaf, and its warnings, handle EDIDs that lie. A fixture PR
is data only; you don't need to write any Rust. The maintainer writes the tests. The corpus
layout and the context file format are described in [`doc/testing.md`](doc/testing.md#fixture-tests).

### 1. Capture the raw EDID

Capture the bytes exactly as the system has them. Don't fix anything: not a wrong
extension count, a bad checksum or a placeholder size. Broken EDIDs are the most valuable
ones.

**Linux**: from the repository root, `cargo run --example capture_fixture`
saves every connected display's EDID to `testdata/valid/` under the right name (see
[`doc/setup.md`](doc/setup.md#capturing-edid-fixtures)). It skips EDIDs that piaf cannot
parse; copy those by hand from `/sys/class/drm/<connector>/edid`.

**Windows**: the EDIDs are in the registry. In PowerShell (if access is denied, run it as
administrator):

```powershell
$out = "$env:USERPROFILE\Desktop\edid"
New-Item -ItemType Directory -Force $out | Out-Null

Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Enum\DISPLAY' | ForEach-Object {
    $id = $_.PSChildName

    Get-ChildItem $_.PSPath | ForEach-Object {
        $edid = (Get-ItemProperty `
            -Path (Join-Path $_.PSPath 'Device Parameters') `
            -Name EDID `
            -ErrorAction SilentlyContinue
        ).EDID

        if ($edid) {
            $instance = $_.PSChildName -replace '[^\w]', '_'
            $file = Join-Path $out ("{0}_{1}.bin" -f $id, $instance)
            [IO.File]::WriteAllBytes($file, $edid)
        }
    }
}
```

The registry keeps an entry for every display that was ever connected, so there may be more
files than screens. The first part of each name is the display's ID (`GSM…` is LG, `DEL…`
is Dell); say in the PR which file belongs to which screen.

**Truncated EDIDs** (the extension count says more blocks than there are): if you can,
capture the same display a second way, e.g. Linux from a live USB. That shows whether the
display or the capture path loses the block.

### 2. Serial numbers

EDIDs contain the display's serial number. If you're fine with it being public, leave it
in. If not, don't open a PR: open an issue asking for a private way to send the files, and
the maintainer will anonymise them, fix the checksums and add them with your context.

### 3. Write the context file

Next to each `.bin`, add a `.toml` with the same name:

```toml
[capture]
contributor = "Your name or handle (project, if any)"
date = "2026-10-22"
method = "Windows registry via PowerShell"   # or "Linux sysfs", "capture_fixture", ...
os = "Windows 11"
connection = "HDMI, direct"                   # or "HDMI via generic driver board", adapters, ...
serials = "original"                          # or "anonymised"

[display]
make_model = "LG OLED42C4"
panel = "42-inch OLED"                        # for a driver board: the panel actually behind it
native_resolution = [3840, 2160]
measured_size_mm = [924, 520]                 # the real active area
size_source = "measured"                      # or "manufacturer spec"
reference = "https://..."                     # where a spec value came from (spec sheet, database page)
running_mode = "3840x2160 @ 144 Hz"           # what the OS actually drives

[notes]
known_issues = [
  "Reports 1600x900 mm, a common TV placeholder, not its size",
]
```

Anything you're unsure of: leave it out or say so in `known_issues`. A guess presented as a
fact is worse than a gap.

### 4. Open the PR

- Put the files in `testdata/`:
  - `valid/`: the EDID is truthful
  - `edge/`: it parses but lies or is odd (placeholder size, a driver board's own EDID,
    0×0 cm in the base block)
  - `invalid/`: piaf rejects it today (e.g. truncated)
- Name them as `capture_fixture` would: the display name from the EDID (or the
  manufacturer ID and product code in hex if it has none), then the manufacturer ID,
  lowercase, words joined with `_`. For example `lg_ultragear_gsm.bin` and
  `lg_ultragear_gsm.toml`. Add a suffix (`_2`) for a second capture of the same model.
- One PR per set of displays (e.g. one machine or cabinet) is fine.
- In the PR description, confirm that the display's owner agrees to publish the files under
  piaf's license (MPL-2.0).
- CI passes as is: the PR adds data, not code.

The maintainer then adds tests for each fixture, and may ask a follow-up question or two in
the PR.

## Coding standards

- Run `cargo fmt` and `cargo clippy -- -D warnings` before pushing.
- Public items need rustdoc comments (`cargo rustdoc -- -D missing_docs` must pass).
- Follow the existing patterns in the codebase — see [`doc/architecture.md`](doc/architecture.md)
  and [`doc/model.md`](doc/model.md) for the design principles behind them.
- `#![deny(unsafe_code)]` is enforced; no unsafe code.
- Keep `no_std` compatibility. The static pipeline and all scalar decoding must compile
  without `alloc` or `std`.

## Commit and PR expectations

- Write commit messages in the imperative mood ("Add support for …", not "Added …").
- Keep commits logically atomic. A PR that touches three unrelated things should be three
  commits (or three PRs).
- Tests are expected for new decoding logic. A unit test with a handcrafted byte slice is
  usually sufficient; a real fixture capture is a bonus.
- CI must be green before a PR can merge: fmt, clippy, docs, all test and build targets,
  and coverage must not drop more than 0.1% below the baseline (stored in
  `.coverage-baseline`). New decoding logic without tests will likely trip this.

## Review process

PRs are reviewed on a best-effort basis. Expect feedback within a few days; if you haven't
heard back in a week feel free to ping the thread. Reviews aim to be constructive — if
something needs to change, the reviewer will explain why. Approval from the maintainer is
required to merge.

## Code of Conduct

This project follows the [Contributor Covenant 3.0](https://github.com/ashlar-display/.github/blob/HEAD/CODE_OF_CONDUCT.md),
the code of conduct of every [Ashlar](https://ashlar-display.org) repository. Please read it
before participating.
