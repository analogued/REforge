# DOS startup and address conventions

The DOS program starts through a self-unpacking loader. Its initialized expanded
image is `0x12b30` bytes (76,592 bytes), with 132 internal segment relocations.
The loader restores the stack and transfers control to relative `0000:0000`.

Use the expanded image before relocation as the address-independent analysis
input. It is a raw memory image, not a runnable DOS executable. The copied
unpacker, uninitialized allocation, and stack storage are outside its bounds.
For a runtime image loaded at segment `L`, relocation adds `L` modulo 65,536
to each inventoried segment word. Runtime addresses must record `L`; a raw dump
must not be interpreted as if its segment words were still relative.

Addresses below are hexadecimal offsets in the expanded program's code segment,
relative to `L`. The initial runtime data segment is `L + 095a`, and the restored
stack is `L + 14fd:0080`.

## Initialization path

| Offset | Role |
| --- | --- |
| `0000` | Runtime entry: sets data/stack segments, clears uninitialized data, and obtains DOS environment and command-line information |
| `02f6` | Calls the runtime wrapper at `8010` |
| `8010` | Prepares runtime state and standard file handles |
| `809d` | Calls application entry `0339` |
| `0339` | Begins application initialization |
| `037f` | Calls buffer/graphics initialization at `5cad` |
| `0392` | Calls the asset loader at `07b5` |

## Initial graphics and file access

Graphics initialization allocates buffers through the DOS allocation helper at
`5c2b`, then calls `5c17`. The BIOS video interrupt at `5c1d` receives `AX=0004`.
This is the verified initial CGA mode-setting path; it does not establish all
hardware modes the program may support.

The asset loader uses DOS file wrappers. Its first file is `data1`, opened
read-only. The first read requests `0x3f40` bytes through DOS function `3f`.
The open wrapper at `81fb` loads the filename pointer at `8202`, sets function
`3d` at `8205`, and uses the shared interrupt site at `81e6`.

The expanded image, relocation handling, entry transfer, initial mode call,
and first open/read request have been checked against an isolated DOSBox-X
debugger execution. The broader startup map is based on static control flow.
See [CGA graphics](cga-graphics.md) for verified asset layouts and composition,
and [DOS opening gameplay](dos-gameplay.md) for the subsequent update loop.
Audio analysis is deferred.
