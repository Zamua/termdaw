import { useState, useEffect, useCallback } from 'react';
import { Box, Text, useInput } from 'ink';
import { useIsFocused, useFocusContext } from '../context/FocusContext.js';
import { useSequencer } from '../context/SequencerContext.js';
import { previewSample, stopPreview } from '../lib/audio.js';
import fs from 'fs';
import path from 'path';

interface FileNode {
  name: string;
  type: 'folder' | 'sample';
  path: string;  // relative path from samples dir
  depth: number;
  children?: FileNode[];
}

function scanDirectory(dirPath: string, relativePath: string = '', depth: number = 0): FileNode[] {
  const items: FileNode[] = [];

  try {
    const entries = fs.readdirSync(dirPath, { withFileTypes: true });

    // Sort: folders first, then files, alphabetically
    const sorted = entries.sort((a, b) => {
      if (a.isDirectory() && !b.isDirectory()) return -1;
      if (!a.isDirectory() && b.isDirectory()) return 1;
      return a.name.localeCompare(b.name);
    });

    for (const entry of sorted) {
      // Skip hidden files
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
      } else if (entry.name.endsWith('.wav') || entry.name.endsWith('.mp3') || entry.name.endsWith('.flac')) {
        items.push({
          name: entry.name,
          type: 'sample',
          path: entryRelPath,
          depth,
        });
      }
    }
  } catch (err) {
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

  // Scan filesystem on mount
  useEffect(() => {
    const scanned = scanDirectory(samplesDir);
    setTree(scanned);
  }, [samplesDir]);

  // Get flattened visible items
  const visibleItems = flattenTree(tree, expandedPaths);

  // Toggle folder expansion
  const toggleFolder = useCallback((folderPath: string) => {
    setExpandedPaths(prev => {
      const next = new Set(prev);
      if (next.has(folderPath)) {
        next.delete(folderPath);
      } else {
        next.add(folderPath);
      }
      return next;
    });
  }, []);

  // Preview sample
  const doPreview = useCallback((item: FileNode) => {
    if (item.type === 'sample') {
      const fullPath = path.join(samplesDir, item.path);
      previewSample(fullPath);
    }
  }, [samplesDir]);

  useInput((input, key) => {
    if (!isFocused) return;

    if (key.upArrow || input === 'k') {
      setSelectedIndex(prev => {
        const newIndex = Math.max(0, prev - 1);
        const item = visibleItems[newIndex];
        if (item?.type === 'sample') {
          doPreview(item);
        }
        return newIndex;
      });
    }
    if (key.downArrow || input === 'j') {
      setSelectedIndex(prev => {
        const newIndex = Math.min(visibleItems.length - 1, prev + 1);
        const item = visibleItems[newIndex];
        if (item?.type === 'sample') {
          doPreview(item);
        }
        return newIndex;
      });
    }

    // Escape to cancel sample selection
    if (key.escape && isSelectingForChannel) {
      stopPreview();
      cancelSampleSelection();
      return;
    }

    // Enter to toggle folder, preview sample, or confirm sample selection
    if (key.return || input === 'o') {
      const item = visibleItems[selectedIndex];
      if (item) {
        if (item.type === 'folder') {
          toggleFolder(item.path);
        } else if (isSelectingForChannel) {
          // Assign sample to channel and return to ChannelRack
          stopPreview();
          const channelIndex = completeSampleSelection();
          if (channelIndex !== null) {
            setChannelSample(channelIndex, item.path);
          }
        } else {
          doPreview(item);
        }
      }
    }

    // l to expand, h to collapse (vim-style)
    if (input === 'l') {
      const item = visibleItems[selectedIndex];
      if (item?.type === 'folder' && !expandedPaths.has(item.path)) {
        toggleFolder(item.path);
      }
    }
    if (input === 'h') {
      const item = visibleItems[selectedIndex];
      if (item?.type === 'folder' && expandedPaths.has(item.path)) {
        // Collapse this folder
        stopPreview();
        toggleFolder(item.path);
      } else if (item && item.depth > 0) {
        // Find parent folder and collapse it, move cursor to parent
        const parentPath = item.path.substring(0, item.path.lastIndexOf('/'));
        if (parentPath && expandedPaths.has(parentPath)) {
          stopPreview();
          toggleFolder(parentPath);
          // Move cursor to the parent folder
          const parentIndex = visibleItems.findIndex(i => i.path === parentPath);
          if (parentIndex >= 0) {
            setSelectedIndex(parentIndex);
          }
        }
      }
    }

    // Space to preview without changing selection
    if (input === ' ') {
      const item = visibleItems[selectedIndex];
      if (item?.type === 'sample') {
        doPreview(item);
      }
    }
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
            const icon = item.type === 'folder'
              ? (isExpanded ? '▼' : '▶')
              : '♪';

            return (
              <Text
                key={item.path}
                color={item.type === 'folder' ? 'yellow' : 'white'}
                backgroundColor={isSelected ? 'blue' : undefined}
                dimColor={index === selectedIndex && !isFocused}
              >
                {indent}{icon} {item.name}
              </Text>
            );
          })
        )}
    </Box>
  );
}
