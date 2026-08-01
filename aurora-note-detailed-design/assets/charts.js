/**
 * Aurora Note 详细设计报告 - 图表初始化
 * 包含：Mermaid 图表渲染 + ECharts 数据图表
 */
(function () {
  'use strict';

  // ===== 1. Mermaid 图表渲染 =====
  if (typeof mermaid !== 'undefined') {
    // 从 CSS 变量获取主题色
    var rootStyle = getComputedStyle(document.documentElement);
    var accent = rootStyle.getPropertyValue('--accent').trim() || '#6c8eef';
    var accent2 = rootStyle.getPropertyValue('--accent2').trim() || '#5ed4a8';
    var bg2 = rootStyle.getPropertyValue('--bg2').trim() || '#1a1d27';
    var ink = rootStyle.getPropertyValue('--ink').trim() || '#e8eaf0';
    var muted = rootStyle.getPropertyValue('--muted').trim() || '#8b91a5';
    var rule = rootStyle.getPropertyValue('--rule').trim() || '#2e3340';

    mermaid.initialize({
      startOnLoad: true,
      theme: 'base',
      themeVariables: {
        primaryColor: bg2,
        primaryTextColor: ink,
        primaryBorderColor: accent,
        lineColor: accent2,
        secondaryColor: bg2,
        tertiaryColor: bg2,
        nodeBorder: accent,
        edgeLabelBackground: bg2,
        clusterBkg: 'transparent',
        clusterBorder: rule,
        fontFamily: 'Instrument Sans, sans-serif',
        fontSize: '14px',
      },
      flowchart: {
        curve: 'basis',
        padding: 20,
        nodeSpacing: 50,
        rankSpacing: 50,
      },
      sequence: {
        actorMargin: 50,
        boxMargin: 10,
      },
    });
  }

  // ===== 2. ECharts 图表渲染 =====
  if (typeof echarts === 'undefined') {
    console.warn('ECharts library not loaded');
    return;
  }

  var style = getComputedStyle(document.documentElement);
  var accent = style.getPropertyValue('--accent').trim() || '#6c8eef';
  var accent2 = style.getPropertyValue('--accent2').trim() || '#5ed4a8';
  var accent3 = style.getPropertyValue('--accent3').trim() || '#f0a35e';
  var accent4 = style.getPropertyValue('--accent4').trim() || '#ef6c6c';
  var accent5 = style.getPropertyValue('--accent5').trim() || '#c084fc';
  var accent6 = style.getPropertyValue('--accent6').trim() || '#38bdf8';
  var ink = style.getPropertyValue('--ink').trim() || '#e8eaf0';
  var muted = style.getPropertyValue('--muted').trim() || '#8b91a5';
  var bg2 = style.getPropertyValue('--bg2').trim() || '#1a1d27';
  var bg3 = style.getPropertyValue('--bg3').trim() || '#232734';
  var rule = style.getPropertyValue('--rule').trim() || '#2e3340';

  // 通用配色
  var palette = [accent, accent2, accent3, accent5, accent6, accent4];

  // 通用文本样式
  var textStyle = {
    color: ink,
    fontFamily: 'Instrument Sans, sans-serif',
    fontSize: 13,
  };

  // 通用 tooltip 样式
  var tooltipStyle = {
    backgroundColor: bg3,
    borderColor: rule,
    borderWidth: 1,
    textStyle: { color: ink, fontSize: 13 },
  };

  // ===== 图表 1: 架构评估雷达图 =====
  var radarEl = document.getElementById('chart-arch-radar');
  if (radarEl) {
    var radarChart = echarts.init(radarEl);
    radarChart.setOption({
      backgroundColor: 'transparent',
      tooltip: { ...tooltipStyle },
      legend: {
        data: ['Aurora Note V15', 'Notion', 'Obsidian', 'Joplin'],
        bottom: 0,
        textStyle: { color: muted, fontSize: 12 },
        itemGap: 20,
      },
      radar: {
        indicator: [
          { name: '性能', max: 10 },
          { name: '可扩展性', max: 10 },
          { name: '可演进性', max: 10 },
          { name: '隐私安全', max: 10 },
          { name: '离线能力', max: 10 },
          { name: '协作能力', max: 10 },
          { name: 'AI 集成', max: 10 },
          { name: '插件生态', max: 10 },
        ],
        center: ['50%', '48%'],
        radius: '65%',
        axisName: { color: ink, fontSize: 13 },
        splitLine: { lineStyle: { color: rule } },
        splitArea: { areaStyle: { color: ['transparent', 'rgba(108,142,239,0.03)'] } },
        axisLine: { lineStyle: { color: rule } },
      },
      series: [{
        type: 'radar',
        data: [
          {
            value: [9.2, 9.2, 9.5, 9.8, 10, 9.0, 9.2, 8.2],
            name: 'Aurora Note V15',
            areaStyle: { color: 'rgba(108,142,239,0.15)' },
            lineStyle: { color: accent, width: 2 },
            itemStyle: { color: accent },
          },
          {
            value: [8.0, 8.5, 7.0, 5.0, 4.0, 9.5, 8.0, 10],
            name: 'Notion',
            lineStyle: { color: accent3, width: 1.5, type: 'dashed' },
            itemStyle: { color: accent3 },
          },
          {
            value: [8.5, 6.0, 7.5, 9.0, 10, 5.0, 5.0, 9.0],
            name: 'Obsidian',
            lineStyle: { color: accent2, width: 1.5, type: 'dashed' },
            itemStyle: { color: accent2 },
          },
          {
            value: [7.0, 5.5, 6.0, 8.5, 9.0, 4.0, 3.0, 5.0],
            name: 'Joplin',
            lineStyle: { color: muted, width: 1.5, type: 'dashed' },
            itemStyle: { color: muted },
          },
        ],
      }],
    });
    window.addEventListener('resize', function () { radarChart.resize(); });
  }

  // ===== 图表 2: CRDT 性能对比柱状图 =====
  var crdtBarEl = document.getElementById('chart-crdt-perf');
  if (crdtBarEl) {
    var crdtBar = echarts.init(crdtBarEl);
    crdtBar.setOption({
      backgroundColor: 'transparent',
      tooltip: { trigger: 'axis', ...tooltipStyle },
      legend: {
        data: ['Loro', 'Yjs', 'Automerge'],
        bottom: 0,
        textStyle: { color: muted },
      },
      grid: { left: '8%', right: '5%', top: '10%', bottom: '18%' },
      xAxis: {
        type: 'category',
        data: ['1K ops', '10K ops', '100K ops', '1M ops'],
        axisLabel: { color: ink },
        axisLine: { lineStyle: { color: rule } },
      },
      yAxis: {
        type: 'value',
        name: '延迟 (ms)',
        nameTextStyle: { color: muted },
        axisLabel: { color: ink },
        splitLine: { lineStyle: { color: rule } },
      },
      series: [
        {
          name: 'Loro',
          type: 'bar',
          data: [0.5, 3.2, 28, 210],
          itemStyle: { color: accent, borderRadius: [4, 4, 0, 0] },
        },
        {
          name: 'Yjs',
          type: 'bar',
          data: [0.8, 5.1, 45, 380],
          itemStyle: { color: accent3, borderRadius: [4, 4, 0, 0] },
        },
        {
          name: 'Automerge',
          type: 'bar',
          data: [1.2, 8.5, 82, 720],
          itemStyle: { color: accent4, borderRadius: [4, 4, 0, 0] },
        },
      ],
    });
    window.addEventListener('resize', function () { crdtBar.resize(); });
  }

  // ===== 图表 3: 同步模式对比 =====
  var syncBarEl = document.getElementById('chart-sync-modes');
  if (syncBarEl) {
    var syncBar = echarts.init(syncBarEl);
    syncBar.setOption({
      backgroundColor: 'transparent',
      tooltip: { trigger: 'axis', ...tooltipStyle },
      legend: {
        data: ['延迟', '吞吐量', '离线能力'],
        bottom: 0,
        textStyle: { color: muted },
      },
      grid: { left: '8%', right: '5%', top: '10%', bottom: '18%' },
      xAxis: {
        type: 'category',
        data: ['C/S 模式', 'P2P 模式', '桥接模式'],
        axisLabel: { color: ink },
        axisLine: { lineStyle: { color: rule } },
      },
      yAxis: {
        type: 'value',
        max: 100,
        axisLabel: { color: ink, formatter: '{value}%' },
        splitLine: { lineStyle: { color: rule } },
      },
      series: [
        {
          name: '延迟',
          type: 'bar',
          data: [95, 90, 75],
          itemStyle: { color: accent, borderRadius: [4, 4, 0, 0] },
        },
        {
          name: '吞吐量',
          type: 'bar',
          data: [85, 70, 90],
          itemStyle: { color: accent2, borderRadius: [4, 4, 0, 0] },
        },
        {
          name: '离线能力',
          type: 'bar',
          data: [40, 95, 90],
          itemStyle: { color: accent3, borderRadius: [4, 4, 0, 0] },
        },
      ],
    });
    window.addEventListener('resize', function () { syncBar.resize(); });
  }

  // ===== 图表 4: 存储架构对比 =====
  var storagePieEl = document.getElementById('chart-storage-pie');
  if (storagePieEl) {
    var storagePie = echarts.init(storagePieEl);
    storagePie.setOption({
      backgroundColor: 'transparent',
      tooltip: { trigger: 'item', ...tooltipStyle },
      legend: {
        orient: 'vertical',
        right: '5%',
        top: 'center',
        textStyle: { color: ink },
      },
      series: [{
        type: 'pie',
        radius: ['40%', '70%'],
        center: ['40%', '50%'],
        avoidLabelOverlap: false,
        label: {
          show: true,
          color: ink,
          formatter: '{b}\n{d}%',
        },
        labelLine: { lineStyle: { color: rule } },
        data: [
          { value: 45, name: 'SQLite (文档数据)', itemStyle: { color: accent } },
          { value: 25, name: 'RocksDB (索引)', itemStyle: { color: accent2 } },
          { value: 15, name: 'LanceDB (向量)', itemStyle: { color: accent3 } },
          { value: 10, name: 'S3 (快照)', itemStyle: { color: accent5 } },
          { value: 5, name: 'Redis (缓存)', itemStyle: { color: accent6 } },
        ],
      }],
    });
    window.addEventListener('resize', function () { storagePie.resize(); });
  }

  // ===== 图表 5: 技术栈依赖关系 =====
  var techGraphEl = document.getElementById('chart-tech-graph');
  if (techGraphEl) {
    var techGraph = echarts.init(techGraphEl);
    techGraph.setOption({
      backgroundColor: 'transparent',
      tooltip: { ...tooltipStyle },
      series: [{
        type: 'graph',
        layout: 'force',
        roam: true,
        force: {
          repulsion: 200,
          edgeLength: [80, 150],
          gravity: 0.1,
        },
        label: {
          show: true,
          color: ink,
          fontSize: 12,
          position: 'right',
        },
        edgeSymbol: ['none', 'arrow'],
        edgeSymbolSize: [0, 8],
        data: [
          { name: 'Aurora Core', symbolSize: 50, itemStyle: { color: accent }, category: 0 },
          { name: 'Loro CRDT', symbolSize: 40, itemStyle: { color: accent2 }, category: 1 },
          { name: 'iroh P2P', symbolSize: 35, itemStyle: { color: accent6 }, category: 1 },
          { name: 'TipTap', symbolSize: 35, itemStyle: { color: accent3 }, category: 2 },
          { name: 'Tauri 2.0', symbolSize: 30, itemStyle: { color: accent5 }, category: 3 },
          { name: 'Capacitor', symbolSize: 30, itemStyle: { color: accent5 }, category: 3 },
          { name: 'SQLite', symbolSize: 28, itemStyle: { color: accent4 }, category: 4 },
          { name: 'RocksDB', symbolSize: 25, itemStyle: { color: accent4 }, category: 4 },
          { name: 'LanceDB', symbolSize: 25, itemStyle: { color: accent4 }, category: 4 },
          { name: 'Tantivy', symbolSize: 25, itemStyle: { color: accent4 }, category: 4 },
          { name: 'Wasmtime', symbolSize: 28, itemStyle: { color: accent3 }, category: 5 },
          { name: 'ONNX Runtime', symbolSize: 28, itemStyle: { color: accent2 }, category: 5 },
        ],
        links: [
          { source: 'Aurora Core', target: 'Loro CRDT' },
          { source: 'Aurora Core', target: 'iroh P2P' },
          { source: 'Aurora Core', target: 'SQLite' },
          { source: 'Aurora Core', target: 'RocksDB' },
          { source: 'Aurora Core', target: 'LanceDB' },
          { source: 'Aurora Core', target: 'Tantivy' },
          { source: 'Aurora Core', target: 'Wasmtime' },
          { source: 'Aurora Core', target: 'ONNX Runtime' },
          { source: 'TipTap', target: 'Loro CRDT' },
          { source: 'Tauri 2.0', target: 'Aurora Core' },
          { source: 'Capacitor', target: 'Aurora Core' },
        ],
        lineStyle: {
          color: rule,
          width: 1.5,
          curveness: 0.1,
          opacity: 0.6,
        },
      }],
    });
    window.addEventListener('resize', function () { techGraph.resize(); });
  }

  // ===== 图表 6: AI 推理延迟对比 =====
  var aiLatencyEl = document.getElementById('chart-ai-latency');
  if (aiLatencyEl) {
    var aiLatency = echarts.init(aiLatencyEl);
    aiLatency.setOption({
      backgroundColor: 'transparent',
      tooltip: { trigger: 'axis', ...tooltipStyle },
      legend: {
        data: ['本地推理', '云端推理'],
        bottom: 0,
        textStyle: { color: muted },
      },
      grid: { left: '8%', right: '5%', top: '10%', bottom: '18%' },
      xAxis: {
        type: 'category',
        data: ['Embed\n(BGE-M3)', 'Generate\n(Qwen2.5-1.5B)', 'Generate\n(Qwen2.5-7B)', 'RAG\n检索+生成'],
        axisLabel: { color: ink, fontSize: 11 },
        axisLine: { lineStyle: { color: rule } },
      },
      yAxis: {
        type: 'value',
        name: '延迟 (ms)',
        nameTextStyle: { color: muted },
        axisLabel: { color: ink },
        splitLine: { lineStyle: { color: rule } },
      },
      series: [
        {
          name: '本地推理',
          type: 'bar',
          data: [45, 800, 0, 1200],
          itemStyle: { color: accent2, borderRadius: [4, 4, 0, 0] },
        },
        {
          name: '云端推理',
          type: 'bar',
          data: [120, 500, 1200, 800],
          itemStyle: { color: accent3, borderRadius: [4, 4, 0, 0] },
        },
      ],
    });
    window.addEventListener('resize', function () { aiLatency.resize(); });
  }
})();
