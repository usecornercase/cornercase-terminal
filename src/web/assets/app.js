(function () {
  'use strict';

  var DARK = {
    background: '#16181d',
    foreground: '#d6d8dc',
    palette: [
      '#1d1f24', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#d6d8dc',
      '#5c6370', '#ef7b84', '#a9d58b', '#efcc8d', '#79bdf2', '#d38ee6', '#6cc7d2', '#ffffff'
    ]
  };
  var LIGHT = {
    background: '#fbfbfa',
    foreground: '#1f2328',
    palette: [
      '#24292f', '#cf222e', '#116329', '#9a6700', '#0969da', '#8250df', '#1b7c83', '#6e7781',
      '#57606a', '#a40e26', '#1a7f37', '#7d4e00', '#218bff', '#a475f9', '#3192aa', '#8c959f'
    ]
  };
  var NAMES = [
    'black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white',
    'brightBlack', 'brightRed', 'brightGreen', 'brightYellow', 'brightBlue', 'brightMagenta', 'brightCyan',
    'brightWhite'
  ];
  var BACKOFF = [0, 1000, 2000, 4000, 8000, 15000];
  var PING_EVERY = 15000;
  var SILENT_FOR = 45000;

  var colours = window.matchMedia('(prefers-color-scheme: light)').matches ? LIGHT : DARK;
  var xtermTheme = { background: colours.background, foreground: colours.foreground, cursor: colours.foreground };
  NAMES.forEach(function (name, i) { xtermTheme[name] = colours.palette[i]; });

  var term = new window.Terminal({
    fontFamily: 'Menlo, "SF Mono", ui-monospace, "DejaVu Sans Mono", "cornercase symbols", monospace',
    fontSize: 13,
    scrollback: 0,
    theme: xtermTheme
  });
  var fit = new window.FitAddon.FitAddon();
  term.loadAddon(fit);
  term.open(document.getElementById('terminal'));
  fit.fit();

  var notice = document.getElementById('notice');
  var noticeText = document.getElementById('notice-text');
  var encoder = new TextEncoder();
  var socket = null;
  var attempt = 0;
  var timer = null;
  var heard = Date.now();
  var stopped = false;
  var why = null;

  function show(text, button) {
    noticeText.textContent = text;
    document.getElementById('notice-button').hidden = !button;
    notice.hidden = false;
  }

  function hide() {
    notice.hidden = true;
  }

  function open() {
    return socket && socket.readyState === WebSocket.OPEN;
  }

  function send(data) {
    if (open()) {
      socket.send(data);
    }
  }

  function sendJson(message) {
    send(JSON.stringify(message));
  }

  function schedule(reason) {
    if (stopped || timer !== null) {
      return;
    }
    var wait = BACKOFF[Math.min(attempt, BACKOFF.length - 1)];
    attempt += 1;
    show(reason + (wait > 0 ? ' · retrying in ' + Math.round(wait / 1000) + ' s' : ' · reconnecting…'), true);
    timer = window.setTimeout(function () {
      timer = null;
      connect();
    }, wait);
  }

  function stop(text) {
    stopped = true;
    show(text, true);
  }

  function ended(message) {
    if (message.reason === 'detached') {
      stop('detached from cornercase');
    } else if (message.reason === 'rejected') {
      stop(message.message || 'cornercase refused this window');
    } else if (message.reason === 'restart') {
      attempt = 0;
      why = 'cornercase is restarting';
    } else if (message.reason === 'shutdown') {
      why = 'cornercase stopped';
    } else if (message.reason === 'gone') {
      why = message.message || 'cornercase is not running';
    }
  }

  function refused() {
    window.fetch('/', { cache: 'no-store', credentials: 'same-origin' }).then(function (response) {
      if (response.status === 401) {
        stop('this browser is no longer paired: open Remote on your computer and scan its code');
      } else if (response.status === 403) {
        stop('this account cannot open cornercase on this computer');
      } else {
        schedule('connection lost');
      }
    }, function () {
      schedule('connection lost');
    });
  }

  function connect() {
    if (timer !== null) {
      window.clearTimeout(timer);
      timer = null;
    }
    if (socket) {
      socket.onclose = null;
      socket.close();
    }
    stopped = false;
    why = null;
    var opened = false;
    var scheme = window.location.protocol === 'https:' ? 'wss://' : 'ws://';
    var ws = new WebSocket(scheme + window.location.host + '/ws');
    ws.binaryType = 'arraybuffer';
    socket = ws;
    ws.onopen = function () {
      opened = true;
      attempt = 0;
      heard = Date.now();
      hide();
      term.reset();
      fit.fit();
      sendJson({
        type: 'hello',
        cols: term.cols,
        rows: term.rows,
        theme: { zone: Intl.DateTimeFormat().resolvedOptions().timeZone, foreground: colours.foreground, background: colours.background, palette: colours.palette }
      });
      term.focus();
    };
    ws.onmessage = function (event) {
      heard = Date.now();
      if (typeof event.data !== 'string') {
        term.write(new Uint8Array(event.data));
        return;
      }
      var message;
      try {
        message = JSON.parse(event.data);
      } catch (error) {
        return;
      }
      if (message && message.type === 'end') {
        ended(message);
      }
    };
    ws.onclose = function () {
      if (socket !== ws) {
        return;
      }
      socket = null;
      if (stopped) {
        return;
      }
      if (!opened) {
        refused();
      } else {
        schedule(why || 'connection lost');
      }
    };
  }

  term.onData(function (data) {
    send(encoder.encode(data));
  });
  term.onBinary(function (data) {
    var bytes = new Uint8Array(data.length);
    for (var i = 0; i < data.length; i += 1) {
      bytes[i] = data.charCodeAt(i) & 0xff;
    }
    send(bytes);
  });
  term.onResize(function (size) {
    sendJson({ type: 'resize', cols: size.cols, rows: size.rows });
  });

  var box = document.getElementById('terminal');
  var viewport = window.visualViewport;
  function layout() {
    if (viewport) {
      var keyboard = window.innerHeight - viewport.height > 80;
      box.style.top = viewport.offsetTop + 'px';
      box.style.height = viewport.height + 'px';
      box.classList.toggle('keyboard', keyboard);
      window.scrollTo(0, 0);
    }
    fit.fit();
  }
  window.addEventListener('resize', layout);
  if (viewport) {
    viewport.addEventListener('resize', layout);
    viewport.addEventListener('scroll', layout);
  }
  layout();
  document.getElementById('notice-button').addEventListener('click', function () {
    attempt = 0;
    connect();
  });
  document.addEventListener('visibilitychange', function () {
    if (document.visibilityState === 'visible' && !open() && !stopped) {
      attempt = 0;
      connect();
    }
  });
  window.setInterval(function () {
    if (!open()) {
      return;
    }
    sendJson({ type: 'ping' });
    if (Date.now() - heard > SILENT_FOR) {
      socket.close();
    }
  }, PING_EVERY);

  var touchY = null;
  var touchX = 0;
  var swiped = 0;

  function screen() {
    return term.element && term.element.querySelector('.xterm-screen');
  }

  function wheel(lines) {
    var target = screen();
    if (!target) {
      return;
    }
    target.dispatchEvent(new WheelEvent('wheel', {
      deltaY: lines,
      deltaMode: WheelEvent.DOM_DELTA_LINE,
      clientX: touchX,
      clientY: touchY,
      bubbles: true,
      cancelable: true
    }));
  }

  window.addEventListener('touchstart', function (event) {
    if (event.touches.length !== 1) {
      touchY = null;
      return;
    }
    touchY = event.touches[0].clientY;
    touchX = event.touches[0].clientX;
    swiped = 0;
  }, { capture: true, passive: true });

  window.addEventListener('touchmove', function (event) {
    if (touchY === null || event.touches.length !== 1 || !notice.hidden && notice.contains(event.target)) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    var target = screen();
    var row = target ? target.clientHeight / term.rows : 16;
    var y = event.touches[0].clientY;
    swiped += touchY - y;
    touchY = y;
    touchX = event.touches[0].clientX;
    while (Math.abs(swiped) >= row) {
      var direction = swiped > 0 ? 1 : -1;
      wheel(direction);
      swiped -= direction * row;
    }
  }, { capture: true, passive: false });

  window.addEventListener('touchend', function () {
    touchY = null;
  }, { capture: true });

  var symbols = document.fonts && document.fonts.load
    ? document.fonts.load('13px "cornercase symbols"', '\ue5ff').catch(function () {})
    : Promise.resolve();
  Promise.race([symbols, new Promise(function (resolve) { setTimeout(resolve, 1000); })]).then(connect);
})();
