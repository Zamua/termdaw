import { useState, useEffect, useCallback } from 'react';
import { Box, Text, useInput } from 'ink';
import { useIsFocused, useFocusContext } from '../context/FocusContext.js';
import { useSequencer } from '../context/SequencerContext.js';
import { previewSample, stopPreview } from '../lib/audio.js';
import { useVim } from '../hooks/useVim.js';
import type { Position, Key } from '../lib/vim/types.js';
import fs from 'fs';
import path from 'path';

interface FileNode {
  name: string;
  type: 'folder' | 'sample';
  path: string;
  depth: number;
  children?: FileNode[];
}

function scanDirectory(dirPath: string, relativePath: string = '', depth: number = 0): FileNode[] {
  const items: FileNode[] = [];

  try {
    const entries = fs.readdirSync(dirPath, { withFileTypes: true });

    const sorted = entries.sort((a, b) => {
      if (a.isDirectory() && !b.isDirectory()) return -1;
      if (!a.isDirectory() && b.isDirectory()) return 1;
      return a.name.localeCompare(b.name);
    });

    for (const entry of sorted) {
      if (entry.name.startsWith('.')) continue;

      const entryRelPath = relativePath ? `${relativePath}/${entry.name}` : entry.name;
      const fullPath = path.join(dirPath, entry.name);

      if (entry.isDirectory()) {
        const children = scanDirectory(fullPath, entryRelPath, depth + 1);
        items.push({
          name: entry.name,
          type: 'folder',
          path: entryRelPath,
          depth,
          children,
        });
      } else if (
        entry.name.endsWith('.wav') ||
        entry.name.endsWith('.mp3') ||
        entry.name.endsWith('.flac')
      ) {
        items.push({
          name: entry.name,
          type: 'sample',
          path: entryRelPath,
          depth,
        });
      }
    }
  } catch {
    // Directory doesn't exist or can't be read
  }

  return items;
}

function flattenTree(nodes: FileNode[], expandedPaths: Set<string>): FileNode[] {
  const result: FileNode[] = [];

  for (const node of nodes) {
    result.push(node);
    if (node.type === 'folder' && node.children && expandedPaths.has(node.path)) {
      result.push(...flattenTree(node.children, expandedPaths));
    }
  }

  return result;
}

export default function Browser() {
  const isFocused = useIsFocused('browser');
  const { sampleSelection, cancelSampleSelection, completeSampleSelection } = useFocusContext();
  const { setChannelSample } = useSequencer();
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [tree, setTree] = useState<FileNode[]>([]);

  const samplesDir = path.join(process.cwd(), 'samples');
  const isSelectingForChannel = sampleSelection.isSelecting;

  useEffect(() => {
    const scanned = scanDirectory(samplesDir);
    setTree(scanned);
  }, [samplesDir]);

  const visibleItems = flattenTree(tree, expandedPaths);

  const toggleFolder = useCallback((folderPath: string) => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(folderPath)) {
        next.delete(folderPath);
      } else {
        next.add(folderPath);
      }
      return next;
    });
  }, []);

  const doPreview = useCallback(
    (item: FileNode) => {
      if (item.type === 'sample') {
        const fullPath = path.join(samplesDir, item.path);
        previewSample(fullPath);
      }
    },
    [samplesDir]
  );

  // Vim hook - 1D list navigation
  const vim = useVim<null>({
    dimensions: { rows: visibleItems.length || 1, cols: 1 },

    getCursor: () => ({ row: selectedIndex, col: 0 }),

    setCursor: (pos: Position) => {
      const newIndex = Math.max(0, Math.min(visibleItems.length - 1, pos.row));
      setSelectedIndex(newIndex);
      const item = visibleItems[newIndex];
      if (item?.type === 'sample') {
        doPreview(item);
      }
    },

    motions: {
      h: (_count, cursor) => {
        // h = collapse folder or go to parent
        const item = visibleItems[cursor.row];
        if (item?.type === 'folder' && expandedPaths.has(item.path)) {
          stopPreview();
          toggleFolder(item.path);
        } else if (item && item.depth > 0) {
          const parentPath = item.path.substring(0, item.path.lastIndexOf('/'));
          if (parentPath && expandedPaths.has(parentPath)) {
            stopPreview();
            toggleFolder(parentPath);
            const parentIndex = visibleItems.findIndex((i) => i.path === parentPath);
            if (parentIndex >= 0) {
              return { position: { row: parentIndex, col: 0 } };
            }
          }
        }
        return { position: cursor };
      },
      l: (_count, cursor) => {
        // l = expand folder
        const item = visibleItems[cursor.row];
        if (item?.type === 'folder' && !expandedPaths.has(item.path)) {
          toggleFolder(item.path);
        }
        return { position: cursor };
      },
      j: (count, cursor) => ({
        position: { row: Math.min(visibleItems.length - 1, cursor.row + count), col: 0 },
      }),
      k: (count, cursor) => ({
        position: { row: Math.max(0, cursor.row - count), col: 0 },
      }),
      gg: (_count, _cursor) => ({
        position: { row: 0, col: 0 },
      }),
      G: (_count, _cursor) => ({
        position: { row: Math.max(0, visibleItems.length - 1), col: 0 },
      }),
    },

    getDataInRange: () => null,
    deleteRange: () => null,
    insertData: () => {},

    onCustomAction: (char: string, key: Key, _count: number) => {
      // Escape to cancel sample selection
      if (key.escape && isSelectingForChannel) {
        stopPreview();
        cancelSampleSelection();
        return true;
      }

      // Enter or o to toggle folder, preview sample, or confirm selection
      if (key.return || char === 'o') {
        const item = visibleItems[selectedIndex];
        if (item) {
          if (item.type === 'folder') {
            toggleFolder(item.path);
          } else if (isSelectingForChannel) {
            stopPreview();
            const channelIndex = completeSampleSelection();
            if (channelIndex !== null) {
              setChannelSample(channelIndex, item.path);
            }
          } else {
            doPreview(item);
          }
        }
        return true;
      }

      // Space to preview without changing selection
      if (char === ' ') {
        const item = visibleItems[selectedIndex];
        if (item?.type === 'sample') {
          doPreview(item);
        }
        return true;
      }

      return false;
    },
  });

  // All input goes through vim
  useInput((input, key) => {
    if (!isFocused) return;

    const inkKey: Key = {
      upArrow: key.upArrow,
      downArrow: key.downArrow,
      leftArrow: key.leftArrow,
      rightArrow: key.rightArrow,
      pageDown: key.pageDown,
      pageUp: key.pageUp,
      return: key.return,
      escape: key.escape,
      ctrl: key.ctrl,
      shift: key.shift,
      tab: key.tab,
      backspace: key.backspace,
      delete: key.delete,
      meta: key.meta,
    };

    vim.handleInput(input, inkKey);
  });

  return (
    <Box flexDirection="column" paddingX={1}>
      {visibleItems.length === 0 ? (
        <Text dimColor>No samples found</Text>
      ) : (
        visibleItems.map((item, index) => {
          const isSelected = index === selectedIndex && isFocused;
          const isExpanded = item.type === 'folder' && expandedPaths.has(item.path);
          const indent = '  '.repeat(item.depth);
          const icon = item.type === 'folder' ? (isExpanded ? '▼' : '▶') : '♪';

          return (
            <Text
              key={item.path}
              color={item.type === 'folder' ? 'yellow' : 'white'}
              backgroundColor={isSelected ? 'blue' : undefined}
              dimColor={index === selectedIndex && !isFocused}
            >
              {indent}
              {icon} {item.name}
            </Text>
          );
        })
      )}
    </Box>
  );
}
