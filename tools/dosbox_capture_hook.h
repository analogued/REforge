// Copyright (c) 2026 analogued.
// Project license not yet selected; see the repository README.
// Authored observer integrated locally into DOSBox-X's GPL-2.0-or-later normal
// CPU core. No upstream source or original game bytes are reproduced here.
#ifndef MACH3_DOSBOX_CAPTURE_HOOK_H
#define MACH3_DOSBOX_CAPTURE_HOOK_H

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>
#include "vga.h"

namespace mach3_capture {
struct Recorder {
    FILE *frames = nullptr, *index = nullptr, *inputs = nullptr, *raw = nullptr;
    FILE *games = nullptr, *ends = nullptr, *controls = nullptr, *replay = nullptr;
    FILE *display = nullptr;
    std::vector<unsigned char> signature;
    unsigned game_cs = 0, game = 0, pass = 0;
    unsigned long long frame_seq = 0, input_seq = 0, control_seq = 0;
    bool enabled = false, identified = false, active = false, initial = false;
    unsigned long long max_frames = 0;
    bool first_game_only = false;

    [[noreturn]] static void fail(const char *message) {
        std::fprintf(stderr, "Mach 3 recorder: %s\n", message);
        std::exit(2);
    }
    static void flush(FILE *file) {
        if (file && (std::fflush(file) || std::ferror(file))) fail("output write failed");
    }
    static FILE *open(const std::string &directory, const char *name) {
        FILE *file = std::fopen((directory + "/" + name).c_str(), "wbx");
        if (!file) fail("cannot exclusively create capture output");
        std::setvbuf(file, nullptr, _IOFBF, 65536);
        return file;
    }
    Recorder() {
        const char *directory = std::getenv("MACH3_CAPTURE_DIR");
        if (!directory || !*directory) return;
        const char *sig = std::getenv("MACH3_CAPTURE_SIGNATURE");
        if (!sig || std::strlen(sig) < 16 || std::strlen(sig) % 2) fail("missing external startup signature");
        for (size_t i = 0; i < std::strlen(sig); i += 2) {
            char byte[3] = {sig[i], sig[i + 1], 0};
            char *end = nullptr;
            unsigned long value = std::strtoul(byte, &end, 16);
            if (*end || value > 255) fail("invalid startup signature");
            signature.push_back(static_cast<unsigned char>(value));
        }
        const std::string dir(directory);
        frames = open(dir, "frames.bin"); index = open(dir, "frames.tsv");
        inputs = open(dir, "inputs.tsv"); raw = open(dir, "raw-inputs.tsv");
        games = open(dir, "games.tsv"); ends = open(dir, "ends.tsv");
        controls = open(dir, "controls.tsv");
        display = open(dir, "display.tsv");
        std::fputs("seq\tgame\tbody\tpass\tkind\tpalette\tmode\n", index);
        std::fputs("seq\tgame\tbody\tpass\tmask\n", inputs);
        std::fputs("seq\tgame\tbody\tpass\tmask\n", raw);
        std::fputs("game\tscript\trng_a\trng_b\trng_c\n", games);
        std::fputs("game\tbody\tquit\n", ends);
        std::fputs("seq\tgame\tbody\tsite\tf1\tf2\tf3\n", controls);
        std::fputs("seq\tcolor_select\tmode_control\n", display);
        if (const char *path = std::getenv("MACH3_REPLAY_INPUTS")) {
            replay = std::fopen(path, "rb");
            if (!replay) fail("cannot open input replay");
            char header[128];
            if (!std::fgets(header, sizeof(header), replay) ||
                std::strcmp(header, "seq\tgame\tbody\tpass\tmask\n")) fail("invalid replay header");
        }
        if (const char *limit = std::getenv("MACH3_CAPTURE_MAX_FRAMES")) {
            char *end = nullptr;
            max_frames = std::strtoull(limit, &end, 10);
            if (!*limit || *end || !max_frames) fail("invalid capture frame limit");
        }
        if (const char *first = std::getenv("MACH3_CAPTURE_FIRST_GAME")) {
            if (std::strcmp(first, "1")) fail("MACH3_CAPTURE_FIRST_GAME must be 1");
            first_game_only = true;
        }
        enabled = true;
    }
    ~Recorder() {
        bool failed = false;
        for (FILE *f : {frames, index, inputs, raw, games, ends, controls, replay, display}) {
            if (f && std::fclose(f)) failed = true;
        }
        if (failed) {
            std::fprintf(stderr, "Mach 3 recorder: output close failed\n");
            std::_Exit(2);
        }
    }
    unsigned word(unsigned offset) const {
        return mem_readw(SegPhys(ds) + offset);
    }
    void frame(unsigned body, unsigned phase, const char *kind) {
        unsigned char visible[16000];
        for (unsigned y = 0; y < 200; ++y) {
            const unsigned address = 0xb8000 + (y & 1) * 8192 + (y / 2) * 80;
            for (unsigned x = 0; x < 80; ++x) visible[y * 80 + x] = mem_readb(address + x);
        }
        if (std::fwrite(visible, 1, sizeof(visible), frames) != sizeof(visible)) fail("frame write failed");
        std::fprintf(display, "%llu\t%u\t%u\n", frame_seq,
                     vga.tandy.color_select, vga.tandy.mode_control);
        std::fprintf(index, "%llu\t%u\t%u\t%u\t%s\t%u\t%u\n", frame_seq++, game,
                     body, phase, kind, (vga.tandy.color_select >> 5) & 1, mem_readb(0x449));
        flush(frames); flush(index); flush(display);
        if (max_frames && frame_seq >= max_frames) std::exit(0);
    }
    void instruction() {
        if (!enabled) return;
        const unsigned ip = reg_ip;
        if (ip == 0x044a && (!identified || SegValue(cs) == game_cs)) {
            for (unsigned i = 0; i < signature.size(); ++i)
                if (mem_readb(SegPhys(cs) + ip + i) != signature[i]) return;
            if (identified) ++game;
            game_cs = SegValue(cs); identified = true; active = true; initial = true; pass = 0;
            std::fprintf(games, "%u\t%u\t%u\t%u\t%u\n", game, word(0xb6ac),
                         word(0x17c1), word(0x184b), word(0x184d));
            flush(games);
            frame(word(0xb692), 0, "initial");
            initial = false;
            return;
        }
        if (!active || SegValue(cs) != game_cs) return;
        if (ip == 0x0452) {
            std::fprintf(ends, "%u\t%u\t%u\n", game, word(0xb692), word(0xb66a));
            flush(ends); active = false;
            if (first_game_only) std::exit(0);
        } else if (ip == 0x04c2) {
            pass = 0;
            if (initial) { frame(word(0xb692), 0, "initial"); initial = false; }
        } else if (ip == 0x228c) {
            if (++pass > 3) fail("unexpected gameplay poll cadence");
            const unsigned body = (word(0xb692) + (pass == 1 ? 1 : 0)) & 65535;
            const unsigned observed = reg_ax;
            unsigned mask = observed & 31;
            if (replay) {
                unsigned long long seq;
                unsigned replay_game, replay_body, replay_pass, replay_mask;
                if (std::fscanf(replay, "%llu\t%u\t%u\t%u\t%u", &seq, &replay_game,
                                &replay_body, &replay_pass, &replay_mask) != 5 ||
                    seq != input_seq || replay_game != game || replay_body != body ||
                    replay_pass != pass || replay_mask > 31) fail("replay input mismatch or exhaustion");
                mask = replay_mask;
                reg_ax = static_cast<uint16_t>(mask);
            }
            std::fprintf(inputs, "%llu\t%u\t%u\t%u\t%u\n", input_seq, game, body, pass, mask);
            std::fprintf(raw, "%llu\t%u\t%u\t%u\t%u\n", input_seq++, game, body, pass, observed);
            flush(inputs); flush(raw);
        } else if (ip == 0x0698 || ip == 0x06dc || ip == 0x0733) {
            const unsigned phase = ip == 0x0698 ? 1 : ip == 0x06dc ? 2 : 3;
            if (phase != pass) fail("render/input phase mismatch");
            frame(word(0xb692), phase, "gameplay");
        } else if (ip == 0x73b2) {
            const unsigned caller = mem_readw(SegPhys(ss) + reg_sp);
            const char *site = caller == 0x2694 ? "body" : caller == 0x228c ?
                (pass == 0 ? "player1" : pass == 1 ? "player2" : "player3") :
                caller == 0x269f ? "pause269f" : caller == 0x26b8 ? "pause26b8" :
                caller == 0x26d1 ? "pause-confirm" : caller == 0x26e2 ? "pause26e2" : "other";
            const unsigned body = (word(0xb692) + (pass == 0 ? 1 : 0)) & 65535;
            std::fprintf(controls, "%llu\t%u\t%u\t%s\t%u\t%u\t%u\n", control_seq++, game,
                         body, site, mem_readb(SegPhys(ds) + 0x17c6 + 0x3b) != 0,
                         mem_readb(SegPhys(ds) + 0x17c6 + 0x3c) != 0,
                         mem_readb(SegPhys(ds) + 0x17c6 + 0x3d) != 0);
            flush(controls);
        } else if (ip == 0x5bac && mem_readw(SegPhys(ss) + reg_bp + 2) == 0) {
            const unsigned caller = mem_readw(SegPhys(ss) + reg_sp);
            for (unsigned phase = 1; phase <= 7; ++phase)
                if (caller == 0x1ef9 + phase * 0x30) frame(word(0xb692), phase, "transition");
        }
    }
};
inline void instruction() {
    static Recorder recorder;
    recorder.instruction();
}
} // namespace mach3_capture
#endif
