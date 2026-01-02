import { useState, useEffect } from "react";
import { Box, Text, useStdout } from "ink";
import { KittyWaveform } from "./KittyWaveform.js";

interface TransportProps {
  isPlaying: boolean;
  bpm: number;
}

// Left section takes roughly 30 characters
const LEFT_SECTION_WIDTH = 32;
// Pixels per terminal column (approximate)
const PIXELS_PER_COLUMN = 8;

export default function Transport({ isPlaying, bpm }: TransportProps) {
  const { stdout } = useStdout();
  const [termWidth, setTermWidth] = useState(stdout?.columns || 120);

  useEffect(() => {
    const handleResize = () => {
      setTermWidth(stdout?.columns || 120);
    };

    stdout?.on("resize", handleResize);
    return () => {
      stdout?.off("resize", handleResize);
    };
  }, [stdout]);

  // Calculate responsive waveform size based on terminal width (max 20 columns)
  const MAX_COLUMNS = 20;
  const availableColumns = Math.min(
    MAX_COLUMNS,
    Math.max(20, termWidth - LEFT_SECTION_WIDTH - 4),
  );
  const waveformPixelWidth = availableColumns * PIXELS_PER_COLUMN;

  return (
    <Box paddingX={1} paddingY={0} justifyContent="space-between">
      {/* Left section: Transport controls */}
      <Box gap={2}>
        {/* Play/Stop indicator */}
        <Box gap={1}>
          <Text color={isPlaying ? "green" : "gray"} bold>
            {isPlaying ? "▶" : "■"}
          </Text>
          <Text color={isPlaying ? "green" : "white"}>
            {isPlaying ? "Playing" : "Stopped"}
          </Text>
        </Box>

        {/* BPM */}
        <Box gap={1}>
          <Text color="cyan" bold>
            {bpm}
          </Text>
          <Text dimColor>BPM</Text>
        </Box>

        {/* Time signature */}
        <Text dimColor>4/4</Text>
      </Box>

      {/* Spacer */}
      <Box flexGrow={1} />

      {/* Right section: Waveform visualizer */}
      <Box marginLeft={2}>
        <KittyWaveform
          width={waveformPixelWidth}
          height={32}
          columns={availableColumns}
          rows={2}
        />
      </Box>
    </Box>
  );
}
