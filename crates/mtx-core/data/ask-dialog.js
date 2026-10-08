// The install prompt (crates/mtx-core/src/consent.rs), run by
// `osascript -l JavaScript -e <this> TITLE INFO ITEMS [check]`.
//
// An alert with the packages in a scrollable outline: each package the
// request is for, expandable to the dependencies it brings along. ITEMS is
// JSON: [{"n": name, "s": size, "d": summary, "c": [children]}]. Prints the
// button pressed, or "timeout" after 30 s. With "check", prints the
// outline's rows instead of showing the alert (tests: no window).
ObjC.import('Cocoa');

function run(argv) {
  const [title, info, json, check] = argv;
  const items = JSON.parse(json);

  // NSOutlineView compares rows by identity, so each row is one NSString,
  // created once: "3" for the fourth package, "3/0" for its first child.
  const rows = {};
  const keys = (list, prefix) => list.map((it, i) => {
    const key = prefix + i;
    rows[key] = { item: it, ns: $.NSString.stringWithString(key), children: keys(it.c || [], key + '/') };
    return key;
  });
  const top = keys(items, '');
  const row = (item) => rows[ObjC.unwrap(item)];
  const kids = (item) => (item.isNil() ? top : row(item).children);

  ObjC.registerSubclass({
    name: 'MTXPackageSource',
    protocols: ['NSOutlineViewDataSource'],
    methods: {
      'outlineView:numberOfChildrenOfItem:': {
        types: ['long', ['id', 'id']],
        implementation: (view, item) => kids(item).length,
      },
      'outlineView:isItemExpandable:': {
        types: ['bool', ['id', 'id']],
        implementation: (view, item) => row(item).children.length > 0,
      },
      'outlineView:child:ofItem:': {
        types: ['id', ['id', 'long', 'id']],
        implementation: (view, index, item) => rows[kids(item)[index]].ns,
      },
      'outlineView:objectValueForTableColumn:byItem:': {
        types: ['id', ['id', 'id', 'id']],
        implementation: (view, column, item) => {
          const it = row(item).item;
          const field = { name: it.n, size: it.s, summary: it.d }[ObjC.unwrap(column.identifier)];
          return $.NSString.stringWithString(field || '');
        },
      },
    },
  });

  const app = $.NSApplication.sharedApplication;
  app.setActivationPolicy($.NSApplicationActivationPolicyAccessory);

  const alert = $.NSAlert.alloc.init;
  alert.messageText = title;
  alert.informativeText = info;
  alert.addButtonWithTitle('Install');
  alert.addButtonWithTitle('Install All');
  alert.addButtonWithTitle("Don't Install").keyEquivalent = '\x1b';

  const height = Math.min(Math.max(top.length * 20 + 26, 70), 260);
  const scroll = $.NSScrollView.alloc.initWithFrame($.NSMakeRect(0, 0, 560, height));
  scroll.hasVerticalScroller = true;
  scroll.autohidesScrollers = true;
  scroll.borderType = $.NSBezelBorder;
  const outline = $.NSOutlineView.alloc.initWithFrame($.NSMakeRect(0, 0, 560, height));
  const column = (id, title, width) => {
    const c = $.NSTableColumn.alloc.initWithIdentifier(id);
    c.title = title;
    c.width = width;
    outline.addTableColumn(c);
    return c;
  };
  outline.outlineTableColumn = column('name', 'Package', 170);
  column('size', 'Size', 70);
  column('summary', 'Description', 300);
  outline.usesAlternatingRowBackgroundColors = true;
  outline.columnAutoresizingStyle = $.NSTableViewLastColumnOnlyAutoresizingStyle;
  const source = $.MTXPackageSource.alloc.init;
  outline.dataSource = source;
  scroll.documentView = outline;
  alert.accessoryView = scroll;
  outline.reloadData;

  if (check === 'check') {
    // The outline's rows, all expanded, one per line ("  " per level).
    outline.expandItemExpandChildren($(), true);
    const lines = [];
    for (let i = 0; i < outline.numberOfRows; i++) {
      const it = row(outline.itemAtRow(i)).item;
      lines.push('  '.repeat(outline.levelForRow(i)) + it.n + ' ' + it.s);
    }
    return lines.join('\n');
  }

  // Give up after 30 s, like `display dialog ... giving up after 30`.
  const timer = $.NSTimer.timerWithTimeIntervalRepeatsBlock(30, false, () => app.abortModal);
  $.NSRunLoop.currentRunLoop.addTimerForMode(timer, $.NSModalPanelRunLoopMode);
  app.activateIgnoringOtherApps(true);
  const answer = alert.runModal;
  timer.invalidate;
  return { 1000: 'Install', 1001: 'Install All', 1002: "Don't Install" }[answer] || 'timeout';
}
