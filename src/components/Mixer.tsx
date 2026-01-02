import { useState } from "react";
import { Box, Text, useInput } from "ink";
import { useIsFocused } from "../context/FocusContext.js";
import { useSequencer } from "../context/SequencerContext.js";

function VolumeBar({
  volume,
  height = 12,
}: {
  volume: number;
  height?: number;
}) {
  const filled = Math.round((volume / 100) * height);

  return (
    <Box flexDirection="column" alignItems="center">
      {Array.from({ length: height }, (_, i) => {
        const level = height - i;
        const isFilled = level <= filled;
        let color: string = "gray";

        if (isFilled) {
          if (level > height * 0.8) color = "red";
          else if (level > height * 0.6) color = "yellow";
          else color = "green";
        }

        return (
          <Text key={i} color={color}>
            {isFilled ? "█" : "░"}
          </Text>
        );
      })}
    </Box>
  );
}

export default function Mixer() {
  const isFocused = useIsFocused("mixer");
  const { channels, toggleMute, toggleSolo, setChannelVolume } = useSequencer();
  const [selectedChannel, setSelectedChannel] = useState(0);

  // Filter to non-empty channels
  const nonEmptyChannels = channels.filter(
    (ch) => ch.sample || ch.type === "synth",
  );

  const channelCount = nonEmptyChannels.length;

  // Handle keyboard input
  useInput(
    (input, key) => {
      if (!isFocused || channelCount === 0) return;

      // Navigation
      if (input === "h" || key.leftArrow) {
        setSelectedChannel((prev) => Math.max(0, prev - 1));
        return;
      }
      if (input === "l" || key.rightArrow) {
        setSelectedChannel((prev) => Math.min(channelCount - 1, prev + 1));
        return;
      }

      // Get actual channel index for the selected channel
      const channel = nonEmptyChannels[selectedChannel];
      if (!channel) return;
      const actualIndex = channels.findIndex((ch) => ch === channel);
      if (actualIndex === -1) return;

      // Mute/Solo toggles
      if (input === "m") {
        toggleMute(actualIndex);
        return;
      }
      if (input === "s") {
        toggleSolo(actualIndex);
        return;
      }

      // Volume adjustment (j/k = 1%, J/K = 5%)
      if (input === "k") {
        setChannelVolume(actualIndex, channel.volume + 1);
        return;
      }
      if (input === "j") {
        setChannelVolume(actualIndex, channel.volume - 1);
        return;
      }
      if (input === "K") {
        setChannelVolume(actualIndex, channel.volume + 5);
        return;
      }
      if (input === "J") {
        setChannelVolume(actualIndex, channel.volume - 5);
        return;
      }

      // Number keys for direct volume
      const num = parseInt(input);
      if (!isNaN(num) && num >= 0 && num <= 9) {
        const newVolume = num === 0 ? 100 : num * 10;
        setChannelVolume(actualIndex, newVolume);
      }
    },
    { isActive: isFocused },
  );

  if (channelCount === 0) {
    return (
      <Box paddingX={1}>
        <Text dimColor>No channels to mix</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Channel strips */}
      <Box flexDirection="row" gap={1}>
        {nonEmptyChannels.map((channel, index) => {
          const isSelected = isFocused && index === selectedChannel;
          return (
            <Box
              key={index}
              flexDirection="column"
              alignItems="center"
              borderStyle="single"
              borderColor={isSelected ? "cyan" : "gray"}
              paddingX={1}
              width={8}
            >
              {/* Volume meter */}
              <VolumeBar volume={channel.volume} />

              {/* Volume value */}
              <Text dimColor>{channel.volume.toString().padStart(3)}%</Text>

              {/* Mute/Solo */}
              <Box gap={1}>
                <Text
                  color={channel.muted ? "red" : "gray"}
                  bold={channel.muted}
                >
                  M
                </Text>
                <Text
                  color={channel.solo ? "yellow" : "gray"}
                  bold={channel.solo}
                >
                  S
                </Text>
              </Box>

              {/* Channel name */}
              <Text color={isSelected ? "cyan" : "white"} bold={isSelected}>
                {channel.name.slice(0, 6)}
              </Text>
            </Box>
          );
        })}
      </Box>

      {/* Help text when focused */}
      {isFocused && (
        <Box marginTop={1}>
          <Text dimColor>
            h/l:Navigate │ m:Mute │ s:Solo │ j/k:±1% │ J/K:±5% │ 0-9:Set
          </Text>
        </Box>
      )}
    </Box>
  );
}
