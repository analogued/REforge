# Analysis tools

`survey_dos_exe.py` reads a DOS MZ executable and writes JSON to standard output.
It records header geometry, relocation targets, a SHA-256 input identity, simple
packing-signature searches, and printable ASCII runs. The default mode uses
only the Python standard library; optional disassembly requires Rizin and
`rzpipe`. It does not run the executable or modify it.

Supply external paths explicitly. For example, from this repository:

```sh
python3 tools/survey_dos_exe.py /path/to/original/CAT.EXE \
    > /path/to/private/analysis/executable-survey.json
```

To include a 64-instruction entry window and an additional inspection window:

```sh
python3 tools/survey_dos_exe.py /path/to/original/CAT.EXE \
    --disassemble --window 0x87da:6 \
    > /path/to/private/analysis/executable-survey.json
```

Create the destination directory first. Keep reports outside the source
repository, and do not redirect output over an input file or existing evidence.
Raw reports can contain original strings and instruction bytes.

Window addresses and reported near-call destinations are file offsets in the
unrelocated input, not DOS runtime addresses. Decode only from independently
identified instruction boundaries. Windows do not establish reachability or
routine boundaries and can extend into data. ASCII runs are not automatically
strings, and absent signatures do not prove an executable is unpacked.
