# What belongs in docs

This directory is for people who use Real Disunity to make games: someone writing a Rust game on the `worldrunner` crate, someone writing C# scripts against `UnityEngine`, or someone trying to run a Unity project. Read this before adding or editing a page here.

## The reader

Write for a game developer who knows Rust or C#, and probably knows Unity, but has never read this engine's source. They want to know what the engine can do, how to get something on screen, and what will not work yet. They do not need to know how a subsystem is implemented, which issue introduced it or how it used to work.

A useful test for any paragraph: would a person building a game change what they do after reading it? If the answer is no, it belongs in [dev-docs](../dev-docs/README.md) or nowhere.

## What goes here

- **How to do something**: install, build, run, write a script, draw a sprite, import a project. Show a complete example that compiles against current `main`.
- **What is supported**: features, API members and settings a game can use today, and the ones it cannot. Say plainly when something is missing.
- **Current state**: what works and what does not, stated as of now, with a link to the tracking issue when work is in progress.
- **Short pointers** to the developer contract for readers who need exact units, ranges or ordering.

## What does not go here

- **Implementation contracts** for engine developers, such as pass ordering, buffer layouts, allocation budgets, ABI regeneration and test strategy. Those go in `dev-docs/`.
- **History**: what changed, what was removed, which PR did it, or how the engine used to behave. Git and the issue tracker hold history.
- **Migration and upgrade guides.** The engine has no stable release and no external users to migrate. When behavior changes, update the page to describe the new behavior.
- **Plans written as fact.** Describe future work only as future work, with a link to its issue.
- **Session notes, receipts and verification logs**, such as "not verified on Windows locally". CI results are the record.

## How to write it

Follow the project writing guide ([acoliver/vibetools, docs/writing/styleguide.md](https://github.com/acoliver/vibetools/blob/main/docs/writing/styleguide.md)). In short: plain, specific sentences; no em dashes; no "not X, but Y" pivots; no empty adjectives such as "powerful" or "seamless"; and lists only where the items are concrete, with prose explaining why the list is there.

Keep examples honest. Every code block in these pages should compile against the current engine. Build it before committing, and update it in the same PR when an API it uses changes. When a page describes a limit, check it against the source; `csharp/Disunity` is the source of truth for the [API list](unity-api.md).

Use one term per concept across all pages: "world" for the native simulation, "host" for `DisunityHost`, "importer" for `unity-import`, and "frame profile" for the legacy or HDR rendering setup.
