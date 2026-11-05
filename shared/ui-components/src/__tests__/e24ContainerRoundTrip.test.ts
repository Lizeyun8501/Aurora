/**
 * E24（TA 22:13）: 容器块序列化往返——callout（GFM alert）/super_block/tabs_block
 * docToMd 语序断言（S2 落库链同款序列化——往返一致性保障）
 */
import { describe, it, expect } from 'vitest';
import { auroraSchema } from '../schema/auroraSchema';
import { docToMd } from '../editors/auroraMarkdown';
import type { Node as ProseNode } from 'prosemirror-model';

const P = (text: string) => auroraSchema.nodes.paragraph.create(null, auroraSchema.text(text));

describe('E24 容器块序列化', () => {
  it('callout → GFM alert [!NOTE]', () => {
    const doc = auroraSchema.nodes.doc.create(null, [
      auroraSchema.nodes.callout.create({ style: 'tip' }, [P('提示内容')]),
    ] as ProseNode[]);
    const md = docToMd(doc);
    expect(md).toContain('[!TIP]');
    expect(md).toContain('> 提示内容');
  });

  it('super_block 横/纵 → HTML 容器', () => {
    const doc = auroraSchema.nodes.doc.create(null, [
      auroraSchema.nodes.super_block.create({ dir: 'h' }, [P('左'), P('右')]),
      auroraSchema.nodes.super_block.create({ dir: 'v' }, [P('上'), P('下')]),
    ] as ProseNode[]);
    const md = docToMd(doc);
    expect(md).toContain('<div class="super-h">');
    expect(md).toContain('<div class="super-v">');
  });

  it('tabs_block → HTML 容器+labels', () => {
    const doc = auroraSchema.nodes.doc.create(null, [
      auroraSchema.nodes.tabs_block.create({ labels: JSON.stringify(['页签一', '页签二']), active: 0 }, [P('内容')]),
    ] as ProseNode[]);
    const md = docToMd(doc);
    expect(md).toContain('<div class="tabs-block"');
    expect(md).toContain('页签一');
  });
});
