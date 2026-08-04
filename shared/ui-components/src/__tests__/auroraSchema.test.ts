import { describe, it, expect } from 'vitest';
import { auroraSchema, AURORA_BLOCK_TYPES } from '../auroraSchema';

describe('auroraSchema', () => {
  it('defines all standard block node types', () => {
    const nodeTypes = Object.keys(auroraSchema.nodes);
    expect(nodeTypes).toContain('doc');
    expect(nodeTypes).toContain('paragraph');
    expect(nodeTypes).toContain('heading');
    expect(nodeTypes).toContain('code');
    expect(nodeTypes).toContain('blockquote');
    expect(nodeTypes).toContain('table');
    expect(nodeTypes).toContain('table_row');
    expect(nodeTypes).toContain('table_cell');
    expect(nodeTypes).toContain('list_item');
    expect(nodeTypes).toContain('divider');
    expect(nodeTypes).toContain('text');
  });

  it('defines all Aurora custom block types', () => {
    const nodeTypes = Object.keys(auroraSchema.nodes);
    expect(nodeTypes).toContain('task_block');
    expect(nodeTypes).toContain('embed');
    expect(nodeTypes).toContain('ai_suggestion');
  });

  it('defines all required marks', () => {
    const markTypes = Object.keys(auroraSchema.marks);
    expect(markTypes).toContain('bold');
    expect(markTypes).toContain('italic');
    expect(markTypes).toContain('code');
    expect(markTypes).toContain('link');
    expect(markTypes).toContain('highlight');
  });

  it('task_block has correct default attributes', () => {
    const taskBlock = auroraSchema.nodes.task_block;
    expect(taskBlock).toBeDefined();
    expect(taskBlock?.attrs).toHaveProperty('taskId');
    expect(taskBlock?.attrs).toHaveProperty('status');
    expect(taskBlock?.attrs).toHaveProperty('priority');
    expect(taskBlock?.attrs).toHaveProperty('dueDate');
  });

  it('embed has required attributes', () => {
    const embed = auroraSchema.nodes.embed;
    expect(embed).toBeDefined();
    expect(embed?.attrs).toHaveProperty('src');
    expect(embed?.attrs).toHaveProperty('type');
  });

  it('ai_suggestion has AI-specific attributes', () => {
    const aiSuggestion = auroraSchema.nodes.ai_suggestion;
    expect(aiSuggestion).toBeDefined();
    expect(aiSuggestion?.attrs).toHaveProperty('suggestionType');
    expect(aiSuggestion?.attrs).toHaveProperty('model');
    expect(aiSuggestion?.attrs).toHaveProperty('accepted');
  });

  it('AURORA_BLOCK_TYPES constants match schema node names', () => {
    expect(AURORA_BLOCK_TYPES.TASK_BLOCK).toBe('task_block');
    expect(AURORA_BLOCK_TYPES.EMBED).toBe('embed');
    expect(AURORA_BLOCK_TYPES.AI_SUGGESTION).toBe('ai_suggestion');
    expect(AURORA_BLOCK_TYPES.PARAGRAPH).toBe('paragraph');
    expect(AURORA_BLOCK_TYPES.HEADING).toBe('heading');
  });

  it('heading supports levels 1-6', () => {
    const heading = auroraSchema.nodes.heading;
    expect(heading).toBeDefined();
    // parseDOM should have entries for h1 through h6
    expect(heading?.parseDOM?.length).toBe(6);
  });

  it('table_cell supports colspan and rowspan', () => {
    const cell = auroraSchema.nodes.table_cell;
    expect(cell).toBeDefined();
    expect(cell?.attrs).toHaveProperty('colspan');
    expect(cell?.attrs).toHaveProperty('rowspan');
  });

  it('highlight mark supports custom color', () => {
    const highlight = auroraSchema.marks.highlight;
    expect(highlight).toBeDefined();
    expect(highlight?.attrs).toHaveProperty('color');
  });
});
