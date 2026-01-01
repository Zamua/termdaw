import { useState, useEffect, useCallback } from 'react';
import { Box, Text, useInput, useStdout } from 'ink';
import { useIsFocused } from '../context/FocusContext.js';

interface Clip {
  patternId: number;
  startBar: number;
  length: number;
}

interface Track {
  name: string;
  clips: Clip[];
  muted: boolean;
}

const NUM_BARS = 16;
const NUM_TRACKS = 99;
const HEADER_ROWS = 5; // Transport, title bar, header, separator, some padding

const createDefaultTracks = (): Track[] =>
  Array.from({ length: NUM_TRACKS }, (_, i) => ({
    name: `Track ${i + 1}`,
    clips: [],
    muted: false,
  }));

export default function Playlist() {
  const isFocused = useIsFocused('playlist');
  const { stdout } = useStdout();
  const [termHeight, setTermHeight] = useState(stdout?.rows || 24);
  const [tracks, setTracks] = useState<Track[]>(createDefaultTracks);
  const [cursorTrack, setCursorTrack] = useState(0);
  const [cursorBar, setCursorBar] = useState(0);
  const [viewportTop, setViewportTop] = useState(0);
  const [playheadBar] = useState(0);
  const [selectedPattern] = useState(1);

  // Track terminal height changes
  useEffect(() => {
    const handleResize = () => {
      setTermHeight(stdout?.rows || 24);
    };
    stdout?.on('resize', handleResize);
    return () => {
      stdout?.off('resize', handleResize);
    };
  }, [stdout]);

  // Calculate viewport height based on terminal size
  const viewportHeight = Math.max(5, termHeight - HEADER_ROWS);

  // Combined cursor movement with viewport scrolling (avoids extra re-renders)
  const moveCursor = useCallback((newTrack: number) => {
    const clampedTrack = Math.max(0, Math.min(tracks.length - 1, newTrack));
    setCursorTrack(clampedTrack);
    setViewportTop(prev => {
      if (clampedTrack < prev) {
        return clampedTrack;
      } else if (clampedTrack >= prev + viewportHeight) {
        return clampedTrack - viewportHeight + 1;
      }
      return prev;
    });
  }, [tracks.length, viewportHeight]);

  useInput((input, key) => {
    // Only handle input when this panel is focused
    if (!isFocused) return;

    // Navigation
    if (key.upArrow || input === 'k') {
      moveCursor(cursorTrack - 1);
      return;
    }
    if (key.downArrow || input === 'j') {
      moveCursor(cursorTrack + 1);
      return;
    }
    // gg - go to first track, G - go to last track
    if (input === 'g') {
      moveCursor(0);
      return;
    }
    if (input === 'G') {
      moveCursor(tracks.length - 1);
      return;
    }
    if (key.leftArrow || input === 'h') {
      setCursorBar(prev => Math.max(0, prev - 1));
      return;
    }
    if (key.rightArrow || input === 'l') {
      setCursorBar(prev => Math.min(NUM_BARS - 1, prev + 1));
      return;
    }

    // Place/remove clip
    if (key.return || input === 'x') {
      setTracks(prev => {
        return prev.map((track, idx) => {
          if (idx !== cursorTrack) return track;

          const existingClipIndex = track.clips.findIndex(
            clip => clip.startBar === cursorBar
          );

          if (existingClipIndex >= 0) {
            // Remove existing clip
            const newClips = [...track.clips];
            newClips.splice(existingClipIndex, 1);
            return { ...track, clips: newClips };
          } else {
            // Add new clip
            return {
              ...track,
              clips: [...track.clips, {
                patternId: selectedPattern,
                startBar: cursorBar,
                length: 1,
              }]
            };
          }
        });
      });
      return;
    }

    // Mute track
    if (input === 'm') {
      setTracks(prev => {
        return prev.map((track, idx) => {
          if (idx !== cursorTrack) return track;
          return { ...track, muted: !track.muted };
        });
      });
      return;
    }
  });

  // Helper to get clip at position
  const getClipAt = (trackIndex: number, bar: number): Clip | undefined => {
    return tracks[trackIndex]?.clips.find(
      clip => clip.startBar <= bar && bar < clip.startBar + clip.length
    );
  };

  return (
    <Box flexDirection="column" paddingX={1}>
      {/* Bar number header */}
      <Box>
        <Box width={10}>
          <Text dimColor>Track</Text>
        </Box>
        <Box width={3}>
          <Text dimColor>M</Text>
        </Box>
        {Array.from({ length: NUM_BARS }, (_, i) => (
          <Box key={`bar-header-${i}`} width={4}>
            <Text
              color={i === playheadBar ? 'green' : i % 4 === 0 ? 'yellow' : 'gray'}
              bold={i === cursorBar && isFocused}
            >
              {String(i + 1).padStart(2, ' ')}
            </Text>
          </Box>
        ))}
      </Box>

      {/* Separator */}
      <Box>
        <Text dimColor>{'─'.repeat(10 + 3 + NUM_BARS * 4)}</Text>
      </Box>

      {/* Track rows - only render viewport */}
      {tracks.slice(viewportTop, viewportTop + viewportHeight).map((track, viewIndex) => {
        const trackIndex = viewportTop + viewIndex;
        const isEmpty = track.clips.length === 0;
        const isCurrentTrack = trackIndex === cursorTrack && isFocused;

        return (
        <Box key={`track-${trackIndex}`}>
          {/* Track name */}
          <Box width={10}>
            <Text
              color={isEmpty && !isCurrentTrack ? 'gray' : track.muted ? 'gray' : isCurrentTrack ? 'cyan' : 'white'}
              bold={isCurrentTrack && !isEmpty}
              dimColor={(track.muted || isEmpty) && !isCurrentTrack}
            >
              {isEmpty ? '(empty)'.padEnd(8) : track.name.slice(0, 8)}
            </Text>
          </Box>

          {/* Mute indicator */}
          <Box width={3}>
            <Text
              color={isEmpty ? 'gray' : track.muted ? 'red' : 'green'}
              dimColor={isEmpty}
            >
              {isEmpty ? '·' : track.muted ? 'M' : '○'}
            </Text>
          </Box>

          {/* Bars */}
          {Array.from({ length: NUM_BARS }, (_, barIndex) => {
            const clip = getClipAt(trackIndex, barIndex);
            const isCursor = trackIndex === cursorTrack && barIndex === cursorBar && isFocused;
            const isPlayhead = barIndex === playheadBar;
            const isBeat = barIndex % 4 === 0;

            let bgColor: string | undefined;
            let fgColor = isEmpty ? 'gray' : 'gray';
            let char = isBeat ? '┃' : '│';

            if (clip) {
              bgColor = track.muted ? 'gray' : 'magenta';
              fgColor = 'white';
              char = `P${clip.patternId}`;
            }

            if (isCursor) {
              bgColor = 'blue';
              fgColor = 'white';
            } else if (isPlayhead) {
              bgColor = clip ? 'green' : undefined;
              fgColor = clip ? 'black' : 'green';
            }

            return (
              <Box key={`bar-${trackIndex}-${barIndex}`} width={4}>
                <Text
                  backgroundColor={bgColor}
                  color={fgColor}
                  bold={!!clip}
                  dimColor={isEmpty && !isCursor && !isPlayhead}
                >
                  {clip ? char.slice(0, 3).padEnd(3, ' ') : char + '  '}
                </Text>
              </Box>
            );
          })}
        </Box>
        );
      })}
    </Box>
  );
}
