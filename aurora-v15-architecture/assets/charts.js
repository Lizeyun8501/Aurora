(function() {
  var style = getComputedStyle(document.documentElement);
  var accent = style.getPropertyValue('--accent').trim();
  var accent2 = style.getPropertyValue('--accent2').trim();
  var accent3 = style.getPropertyValue('--accent3').trim();
  var accent4 = style.getPropertyValue('--accent4').trim();
  var accent5 = style.getPropertyValue('--accent5').trim();
  var accent6 = style.getPropertyValue('--accent6').trim();
  var ink = style.getPropertyValue('--ink').trim();
  var muted = style.getPropertyValue('--muted').trim();
  var rule = style.getPropertyValue('--rule').trim();
  var bg2 = style.getPropertyValue('--bg2').trim();

  // ===== 1. Tech Stack Radar (8 dimensions) =====
  var radarEl = document.getElementById('chart-radar');
  if (radarEl && typeof echarts !== 'undefined') {
    var radarChart = echarts.init(radarEl, null, { renderer: 'canvas' });
    radarChart.setOption({
      backgroundColor: 'transparent',
      animation: false,
      tooltip: { trigger: 'item', backgroundColor: bg2, borderColor: rule, textStyle: { color: ink, fontSize: 13 } },
      radar: {
        indicator: [
          { name: '数据安全', max: 10 },
          { name: '协同性能', max: 10 },
          { name: '跨平台一致性', max: 10 },
          { name: '离线可用性', max: 10 },
          { name: '扩展生态', max: 10 },
          { name: '搜索质量', max: 10 },
          { name: '移动端性能', max: 10 },
          { name: '运维成本', max: 10 }
        ],
        splitArea: { areaStyle: { color: ['rgba(108,142,239,0.02)', 'rgba(108,142,239,0.05)'] } },
        splitLine: { lineStyle: { color: rule } },
        axisLine: { lineStyle: { color: rule } },
        axisName: { color: muted, fontSize: 12 }
      },
      series: [{
        type: 'radar',
        data: [{
          value: [10, 9, 10, 10, 9, 9, 8, 8],
          name: 'V12 技术栈',
          areaStyle: { color: 'rgba(108,142,239,0.15)' },
          lineStyle: { color: accent, width: 2 },
          itemStyle: { color: accent }
        }]
      }]
    });
    window.addEventListener('resize', function() { radarChart.resize(); });
  }

  // ===== 2. Framework 35-Point Scoring (7 frameworks) =====
  var fwEl = document.getElementById('chart-framework-score');
  if (fwEl && typeof echarts !== 'undefined') {
    var fwChart = echarts.init(fwEl, null, { renderer: 'canvas' });
    fwChart.setOption({
      backgroundColor: 'transparent',
      animation: false,
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' }, backgroundColor: bg2, borderColor: rule, textStyle: { color: ink, fontSize: 13 } },
      grid: { left: '12%', right: '5%', bottom: '15%', top: '10%' },
      xAxis: {
        type: 'category',
        data: ['Electron\n+Web', 'Tauri\n+Web', 'Flutter\n+Rust', 'React\nNative', 'Kotlin\nMP', '.NET\nMAUI', 'Rust\n核心'],
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 10, lineHeight: 14 }
      },
      yAxis: {
        type: 'value', max: 35, name: '评分', nameTextStyle: { color: muted, fontSize: 11 },
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 11 },
        splitLine: { lineStyle: { color: rule, type: 'dashed' } }
      },
      series: [{
        name: '综合评分',
        type: 'bar',
        data: [
          { value: 21, itemStyle: { color: accent4 } },
          { value: 29, itemStyle: { color: accent } },
          { value: 28, itemStyle: { color: accent2 } },
          { value: 20, itemStyle: { color: accent3 } },
          { value: 20, itemStyle: { color: accent3 } },
          { value: 19, itemStyle: { color: accent4 } },
          { value: 28, itemStyle: { color: accent2 } }
        ],
        barWidth: '50%',
        label: { show: true, position: 'top', color: ink, fontSize: 12, fontWeight: 700 }
      }]
    });
    window.addEventListener('resize', function() { fwChart.resize(); });
  }

  // ===== 3. Function Coverage Radar (key solutions) =====
  var covEl = document.getElementById('chart-coverage-radar');
  if (covEl && typeof echarts !== 'undefined') {
    var covChart = echarts.init(covEl, null, { renderer: 'canvas' });
    covChart.setOption({
      backgroundColor: 'transparent',
      animation: false,
      tooltip: { trigger: 'item', backgroundColor: bg2, borderColor: rule, textStyle: { color: ink, fontSize: 13 } },
      legend: { textStyle: { color: muted, fontSize: 11 }, top: 5 },
      radar: {
        indicator: [
          { name: '块级编辑+双链', max: 5 },
          { name: 'Canvas画布', max: 5 },
          { name: '对象化数据模型', max: 5 },
          { name: '数据库视图', max: 5 },
          { name: 'FSRS闪卡', max: 5 },
          { name: 'AI/语义搜索', max: 5 },
          { name: 'WASM插件', max: 5 },
          { name: 'P2P同步', max: 5 },
          { name: 'Web端演进', max: 5 },
          { name: '移动UI复用', max: 5 },
          { name: '架构简洁性', max: 5 },
          { name: '端到端加密', max: 5 }
        ],
        splitArea: { areaStyle: { color: ['rgba(108,142,239,0.02)', 'rgba(108,142,239,0.04)'] } },
        splitLine: { lineStyle: { color: rule } },
        axisLine: { lineStyle: { color: rule } },
        axisName: { color: muted, fontSize: 10 }
      },
      series: [{
        type: 'radar',
        data: [
          { value: [5,5,5,5,5,5,5,5,5,5,5,5], name: 'V12', areaStyle: { color: 'rgba(108,142,239,0.2)' }, lineStyle: { color: accent, width: 2 } },
          { value: [5,5,5,5,5,5,5,5,2,0,3,4], name: 'Aurora v3', areaStyle: { color: 'rgba(239,107,107,0.05)' }, lineStyle: { color: accent4, width: 1, type: 'dashed' } },
          { value: [5,0,0,0,2,0,3,0,5,5,5,3], name: 'LocalFirst', areaStyle: { color: 'rgba(240,163,94,0.05)' }, lineStyle: { color: accent3, width: 1, type: 'dashed' } },
          { value: [5,3,4,4,0,5,4,4,2,0,4,4], name: 'LuminaNote', areaStyle: { color: 'rgba(192,132,252,0.03)' }, lineStyle: { color: accent5, width: 1, type: 'dashed' } }
        ]
      }]
    });
    window.addEventListener('resize', function() { covChart.resize(); });
  }

  // ===== 4. Loro vs Yrs Performance (B4 benchmark, log scale) =====
  var loroEl = document.getElementById('chart-loro-vs-yrs');
  if (loroEl && typeof echarts !== 'undefined') {
    var loroChart = echarts.init(loroEl, null, { renderer: 'canvas' });
    loroChart.setOption({
      backgroundColor: 'transparent',
      animation: false,
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' }, backgroundColor: bg2, borderColor: rule, textStyle: { color: ink, fontSize: 13 } },
      legend: { data: ['Loro v1.13.8', 'Yjs (Yjs Rust)'], textStyle: { color: muted, fontSize: 11 }, top: 5 },
      grid: { left: '12%', right: '5%', bottom: '12%', top: '18%' },
      xAxis: {
        type: 'category',
        data: ['Apply (ms)', 'Encode (ms)', 'Parse (ms)', '文档体积 (KB)'],
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 10, rotate: 15 }
      },
      yAxis: {
        type: 'log', name: '对数刻度', nameTextStyle: { color: muted, fontSize: 11 },
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 11 },
        splitLine: { lineStyle: { color: rule, type: 'dashed' } }
      },
      series: [
        { name: 'Loro v1.13.8', type: 'bar', data: [125, 3.7, 18.6, 113], itemStyle: { color: accent }, barWidth: '25%', label: { show: true, position: 'top', color: ink, fontSize: 10, formatter: function(p) { return p.value; } } },
        { name: 'Yjs (Yjs Rust)', type: 'bar', data: [918, 28.4, 152, 160], itemStyle: { color: accent3 }, barWidth: '25%', label: { show: true, position: 'top', color: ink, fontSize: 10, formatter: function(p) { return p.value; } } }
      ]
    });
    window.addEventListener('resize', function() { loroChart.resize(); });
  }

  // ===== 5. Valkey 8.1 vs Redis 8.0 =====
  var valkeyEl = document.getElementById('chart-valkey-benchmark');
  if (valkeyEl && typeof echarts !== 'undefined') {
    var valkeyChart = echarts.init(valkeyEl, null, { renderer: 'canvas' });
    valkeyChart.setOption({
      backgroundColor: 'transparent',
      animation: false,
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' }, backgroundColor: bg2, borderColor: rule, textStyle: { color: ink, fontSize: 13 } },
      legend: { data: ['Valkey 8.1', 'Redis 8.0'], textStyle: { color: muted, fontSize: 12 }, top: 5 },
      grid: { left: '12%', right: '5%', bottom: '12%', top: '18%' },
      xAxis: {
        type: 'category',
        data: ['SET RPS (万)', 'GET RPS (万)', '内存/5000万键 (GB)'],
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 11, rotate: 15 }
      },
      yAxis: {
        type: 'value',
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 11 },
        splitLine: { lineStyle: { color: rule, type: 'dashed' } }
      },
      series: [
        { name: 'Valkey 8.1', type: 'bar', data: [99.98, 105, 3.77], itemStyle: { color: accent2 }, barWidth: '25%', label: { show: true, position: 'top', color: ink, fontSize: 10 } },
        { name: 'Redis 8.0', type: 'bar', data: [72.94, 86, 5.24], itemStyle: { color: accent4 }, barWidth: '25%', label: { show: true, position: 'top', color: ink, fontSize: 10 } }
      ]
    });
    window.addEventListener('resize', function() { valkeyChart.resize(); });
  }

  // ===== 6. Development Roadmap Gantt =====
  var ganttEl = document.getElementById('chart-roadmap-gantt');
  if (ganttEl && typeof echarts !== 'undefined') {
    var ganttChart = echarts.init(ganttEl, null, { renderer: 'canvas' });
    var phases = [
      { name: 'Phase 1: 三层框架', start: 0, duration: 5, color: accent },
      { name: 'Phase 2: 编辑器+知识', start: 5, duration: 5, color: accent2 },
      { name: 'Phase 3: 云同步+AI', start: 10, duration: 6, color: accent3 },
      { name: 'Phase 4: 高级功能', start: 16, duration: 5, color: accent5 },
      { name: 'Phase 5: 生态+Web', start: 21, duration: 4, color: accent6 }
    ];
    ganttChart.setOption({
      backgroundColor: 'transparent',
      animation: false,
      tooltip: {
        trigger: 'item',
        backgroundColor: bg2, borderColor: rule, textStyle: { color: ink, fontSize: 13 },
        formatter: function(p) { return p.data.name + '<br/>第 ' + p.data.start + '-' + (p.data.start + p.data.duration) + ' 周'; }
      },
      grid: { left: '25%', right: '5%', bottom: '10%', top: '10%' },
      xAxis: {
        type: 'value', name: '周', max: 30,
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 11 },
        splitLine: { lineStyle: { color: rule, type: 'dashed' } }
      },
      yAxis: {
        type: 'category',
        data: phases.map(function(p) { return p.name; }).reverse(),
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 11 }
      },
      series: [{
        type: 'custom',
        renderItem: function(params, api) {
          var cat = api.value(0);
          var start = api.coord([api.value(1), cat]);
          var end = api.coord([api.value(2), cat]);
          var height = api.size([0, 1])[1] * 0.6;
          return {
            type: 'rect',
            shape: { x: start[0], y: start[1] - height / 2, width: end[0] - start[0], height: height },
            style: { fill: api.value(3), opacity: 0.8 }
          };
        },
        data: phases.map(function(p, i) {
          return { value: [i, p.start, p.start + p.duration, p.color], name: p.name, start: p.start, duration: p.duration };
        }).reverse()
      }]
    });
    window.addEventListener('resize', function() { ganttChart.resize(); });
  }

  // ===== 7. Audit Score Comparison =====
  var auditEl = document.getElementById('chart-audit-scores');
  if (auditEl && typeof echarts !== 'undefined') {
    var auditChart = echarts.init(auditEl, null, { renderer: 'canvas' });
    auditChart.setOption({
      backgroundColor: 'transparent',
      animation: false,
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' }, backgroundColor: bg2, borderColor: rule, textStyle: { color: ink, fontSize: 13 } },
      legend: { data: ['V8 评分', 'V12 修正后'], textStyle: { color: muted, fontSize: 12 }, top: 5 },
      grid: { left: '15%', right: '5%', bottom: '10%', top: '18%' },
      xAxis: {
        type: 'category',
        data: ['高性能', '可扩展性', '可演进性', '隐私安全性'],
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 12 }
      },
      yAxis: {
        type: 'value', max: 5,
        axisLine: { lineStyle: { color: rule } },
        axisLabel: { color: muted, fontSize: 11 },
        splitLine: { lineStyle: { color: rule, type: 'dashed' } }
      },
      series: [
        { name: 'V8 评分', type: 'bar', data: [4.0, 4.5, 4.5, 4.0], itemStyle: { color: accent3 }, barWidth: '25%' },
        { name: 'V12 修正后', type: 'bar', data: [4.5, 4.5, 5.0, 4.5], itemStyle: { color: accent }, barWidth: '25%' }
      ]
    });
    window.addEventListener('resize', function() { auditChart.resize(); });
  }
})();
