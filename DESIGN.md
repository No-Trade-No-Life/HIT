# HIT Design System

## Scene and strategy

深夜的交易者在单一高分辨率工作屏前确认真实执行状态：页面以纯白工作面保持信息清醒，深橄榄只用于已选择的区域与主操作。采用克制策略，品牌色不超过操作和选择状态所需的比例。

## Color tokens

```css
:root {
  --background: oklch(1 0 0);
  --surface: oklch(0.982 0.006 110);
  --foreground: oklch(0.20 0.018 110);
  --muted-foreground: oklch(0.46 0.024 110);
  --primary: oklch(0.39 0.10 110);
  --primary-foreground: oklch(0.99 0.002 110);
  --accent: oklch(0.57 0.14 235);
  --destructive: oklch(0.52 0.19 28);
  --border: oklch(0.90 0.012 110);
}
```

Success, warning and failure use the system semantic badge variants, together with text labels and icons.

## Typography

Use Geist through the Nova preset's system stack. The product scale is compact: page headings are 24px, section headings 16px, body 14px, and tabular details 13px. IDs, tokens and JSON use the mono face.

## Layout and components

The shell follows OpenAI-LB: a responsive Sidebar for primary resources, a slim top bar for context and account controls, and a generous content inset. Lists are tables at desktop widths and compact labelled rows on small screens. Creation and destructive actions use titled Base UI dialogs; empty states teach the next task.

Use shadcn Base UI primitives only for shared controls. Cards have an 8px radius and a border or a small defined shadow, never both as decoration. Motion stays between 150–200ms and only communicates an operation or data refresh.
