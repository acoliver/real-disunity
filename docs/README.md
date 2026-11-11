# Real Disunity documentation

These pages are for people making games with Real Disunity. Read [the project README](../README.md) first for what the engine is and what works today.

If you are new, read the pages in this order:

1. [Getting started](getting-started.md): install the prerequisites, run the demos and the tests.
2. [Writing a Rust game](rust-games.md): build a world, advance it, and draw it in a window.
3. [Writing C# scripts](csharp-games.md): write `MonoBehaviour` scripts against `UnityEngine` names and host them.
4. [Importing Unity projects](unity-projects.md): what the importer reads and how to inspect a project.
5. [Lua scripts](lua-scripts.md): the sandboxed Lua host, and patching a game's original scripts.

Reference pages:

- [Supported UnityEngine API](unity-api.md): every type and member `Disunity.Engine` implements.
- [Rendering features](rendering.md): what the 2D and 3D renderer can draw and which effects it supports.
- [Drawing with your own shader programs](render-program-native.md): register a WGSL or SPIR-V program, make a material, set its properties and features by name, and draw it.

Documentation for changing the engine itself, including exact subsystem contracts, is in [dev-docs](../dev-docs/README.md). [What belongs in docs](what-belongs-here.md) explains how these pages are written and what goes where.
