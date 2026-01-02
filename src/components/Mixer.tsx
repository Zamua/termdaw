import { Box, Text } from "ink";

interface MixerChannel {
  name: string;
  volume: number;
  pan: number;
  muted: boolean;
  solo: boolean;
}

const defaultChannels: MixerChannel[] = [
  { name: "Master", volume: 80, pan: 0, muted: false, solo: false },
  { name: "Kick", volume: 75, pan: 0, muted: false, solo: false },
  { name: "Snare", volume: 70, pan: 0, muted: false, solo: false },
  { name: "HiHat", volume: 65, pan: 10, muted: false, solo: false },
  { name: "OpenHat", volume: 60, pan: -10, muted: false, solo: false },
  { name: "Crash", volume: 55, pan: 20, muted: false, solo: false },
  { name: "Tom Hi", volume: 65, pan: 15, muted: false, solo: false },
  { name: "Tom Lo", volume: 65, pan: -15, muted: false, solo: false },
];

function VolumeBar({ volume }: { volume: number }) {
  const maxHeight = 6;
  const filled = Math.round((volume / 100) * maxHeight);

  return (
    <Box flexDirection="column" alignItems="center">
      {Array.from({ length: maxHeight }, (_, i) => {
        const level = maxHeight - i;
        const isFilled = level <= filled;
        let color: string = "gray";

        if (isFilled) {
          if (level > maxHeight * 0.8) color = "red";
          else if (level > maxHeight * 0.6) color = "yellow";
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
  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Channel strips */}
      <Box flexDirection="row" gap={1}>
        {defaultChannels.map((channel, index) => (
          <Box
            key={index}
            flexDirection="column"
            alignItems="center"
            borderStyle="single"
            borderColor={index === 0 ? "yellow" : "gray"}
            paddingX={1}
            width={8}
          >
            {/* Volume meter */}
            <VolumeBar volume={channel.volume} />

            {/* Volume value */}
            <Text dimColor>{channel.volume}%</Text>

            {/* Mute/Solo */}
            <Box gap={1}>
              <Text color={channel.muted ? "red" : "gray"}>M</Text>
              <Text color={channel.solo ? "yellow" : "gray"}>S</Text>
            </Box>

            {/* Channel name */}
            <Text color={index === 0 ? "yellow" : "white"} bold={index === 0}>
              {channel.name.slice(0, 6)}
            </Text>
          </Box>
        ))}
      </Box>
    </Box>
  );
}
