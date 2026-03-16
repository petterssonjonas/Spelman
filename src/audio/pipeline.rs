// Phase 1: Minimal pipeline — decode → volume → ring buffer.
// This module will grow in later phases to include:
// decode → resample → normalize → EQ → volume → FFT tap → ring buffer
//
// For now, the pipeline is handled directly in engine.rs.
// This file serves as a placeholder for the full DSP chain.
