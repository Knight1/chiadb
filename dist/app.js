(function () {
  "use strict";

  var state = {
    payload: null,
    allDays: [],
    visibleDays: [],
    range: "30"
  };

  var chartSize = {
    width: 1000,
    footprint: { top: 26, right: 27, bottom: 43, left: 78, height: 350 },
    activity: { top: 25, right: 27, bottom: 34, left: 70, height: 270 }
  };

  function byId(id) {
    return document.getElementById(id);
  }

  function setText(id, value) {
    var element = byId(id);
    if (element) {
      element.textContent = value;
    }
  }

  function escapeHtml(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }

  function comma(value) {
    return new Intl.NumberFormat("en-US").format(Math.round(value));
  }

  function bytes(value) {
    if (!Number.isFinite(value)) {
      return "--";
    }
    var units = ["B", "KiB", "MiB", "GiB", "TiB"];
    var index = 0;
    var number = value;
    while (number >= 1024 && index < units.length - 1) {
      number /= 1024;
      index += 1;
    }
    return number.toFixed(number >= 100 || index === 0 ? 0 : 1) + " " + units[index];
  }

  function signedBytes(value) {
    var prefix = value > 0 ? "+" : value < 0 ? "-" : "";
    return prefix + bytes(Math.abs(value));
  }

  function signedNumber(value) {
    return (value > 0 ? "+" : "") + comma(value);
  }

  function toGiB(value) {
    return value / 1024 / 1024 / 1024;
  }

  function dayDate(day) {
    return new Date(day.date + "T00:00:00Z");
  }

  function shortDate(day) {
    return new Intl.DateTimeFormat("en-US", {
      month: "short",
      day: "numeric",
      timeZone: "UTC"
    }).format(dayDate(day));
  }

  function fullDate(day) {
    return new Intl.DateTimeFormat("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
      timeZone: "UTC"
    }).format(dayDate(day));
  }

  function rangeDays() {
    if (state.range === "all") {
      return state.allDays.slice();
    }
    return state.allDays.slice(-Number(state.range));
  }

  function humanRange() {
    return state.range === "all" ? "Selected window" : state.range + " day";
  }

  function updateRangeButtons() {
    var buttons = document.querySelectorAll("[data-range]");
    buttons.forEach(function (button) {
      var selected = button.getAttribute("data-range") === state.range;
      button.classList.toggle("is-active", selected);
      button.setAttribute("aria-pressed", selected ? "true" : "false");
    });
  }

  function niceStep(value) {
    if (!value || !Number.isFinite(value)) {
      return 1;
    }
    var power = Math.pow(10, Math.floor(Math.log10(value)));
    var normalized = value / power;
    var step = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
    return step * power;
  }

  function niceDomain(values) {
    var min = Math.min.apply(null, values);
    var max = Math.max.apply(null, values);
    var spread = Math.max(max - min, Math.max(max * 0.04, 1));
    var step = niceStep(spread / 4);
    return {
      min: Math.floor((min - step * 0.35) / step) * step,
      max: Math.ceil((max + step * 0.35) / step) * step,
      step: step
    };
  }

  function xPosition(index, length, left, right) {
    if (length <= 1) {
      return left;
    }
    return left + ((chartSize.width - left - right) * index) / (length - 1);
  }

  function yPosition(value, domain, top, bottom, height) {
    var available = height - top - bottom;
    return top + ((domain.max - value) / (domain.max - domain.min)) * available;
  }

  function makeStepLine(points) {
    if (!points.length) {
      return "";
    }
    var path = "M " + points[0].x + " " + points[0].y;
    for (var index = 1; index < points.length; index += 1) {
      path += " H " + points[index].x + " V " + points[index].y;
    }
    return path;
  }

  function chartDateTicks(days, left, right, bottomY) {
    var positions = days.length < 8 ? [0, days.length - 1] : [0, Math.round((days.length - 1) / 2), days.length - 1];
    var unique = positions.filter(function (value, index, array) {
      return array.indexOf(value) === index;
    });
    return unique.map(function (index) {
      var anchor = index === 0 ? "start" : index === days.length - 1 ? "end" : "middle";
      var x = xPosition(index, days.length, left, right);
      return '<text class="date-label" x="' + x + '" y="' + bottomY + '" text-anchor="' + anchor + '">' + escapeHtml(shortDate(days[index])) + "</text>";
    }).join("");
  }

  function renderFootprint(days) {
    var svg = byId("footprint-chart");
    var frame = byId("footprint-frame");
    var tooltip = byId("footprint-tooltip");
    if (!svg || !frame || !tooltip) {
      return;
    }
    if (!days.length) {
      renderEmptyChart(svg, "No database-size samples in this range.", chartSize.footprint.height);
      return;
    }

    var settings = chartSize.footprint;
    var values = days.map(function (day) { return toGiB(day.db_bytes); });
    var domain = niceDomain(values);
    var bottomLine = settings.height - settings.bottom;
    var points = days.map(function (day, index) {
      return {
        x: xPosition(index, days.length, settings.left, settings.right),
        y: yPosition(toGiB(day.db_bytes), domain, settings.top, settings.bottom, settings.height)
      };
    });
    var grid = "";
    for (var tick = domain.min; tick <= domain.max + domain.step * 0.1; tick += domain.step) {
      var y = yPosition(tick, domain, settings.top, settings.bottom, settings.height);
      grid += '<line class="grid-line" x1="' + settings.left + '" x2="' + (chartSize.width - settings.right) + '" y1="' + y + '" y2="' + y + '"></line>';
      grid += '<text class="axis-label" x="' + (settings.left - 13) + '" y="' + (y + 7) + '" text-anchor="end">' + Math.round(tick) + " GiB</text>";
    }

    var line = makeStepLine(points);
    var area = line + " L " + points[points.length - 1].x + " " + bottomLine + " L " + points[0].x + " " + bottomLine + " Z";
    var latestPoint = points[points.length - 1];
    var dateTicks = chartDateTicks(days, settings.left, settings.right, settings.height - 12);

    svg.innerHTML =
      '<defs><linearGradient id="footprint-fill" x1="0" x2="0" y1="0" y2="1"><stop offset="0%" stop-color="#1473e6" stop-opacity="0.36"></stop><stop offset="100%" stop-color="#1473e6" stop-opacity="0.018"></stop></linearGradient></defs>' +
      grid +
      '<line class="axis-line" x1="' + settings.left + '" x2="' + (chartSize.width - settings.right) + '" y1="' + bottomLine + '" y2="' + bottomLine + '"></line>' +
      '<path class="area-fill" d="' + area + '"></path>' +
      '<path class="footprint-line" d="' + line + '"></path>' +
      '<circle class="end-point" cx="' + latestPoint.x + '" cy="' + latestPoint.y + '" r="6"></circle>' +
      dateTicks;

    attachTooltip(svg, frame, tooltip, days, settings.left, settings.right, function (day) {
      return '<span>' + escapeHtml(fullDate(day)) + '</span><strong>' + bytes(day.db_bytes) + '</strong><span>main file</span><br><span>WAL: ' + bytes(day.wal_bytes) + '</span>';
    }, function (day) {
      return yPosition(toGiB(day.db_bytes), domain, settings.top, settings.bottom, settings.height);
    });
  }

  function renderActivity(days) {
    var svg = byId("activity-chart");
    var frame = byId("activity-frame");
    var tooltip = byId("activity-tooltip");
    if (!svg || !frame || !tooltip) {
      return;
    }
    if (!days.length) {
      renderEmptyChart(svg, "No complete coin-activity days in this range.", chartSize.activity.height);
      return;
    }

    var settings = chartSize.activity;
    var baseline = Math.round((settings.top + (settings.height - settings.bottom)) / 2);
    var availableHeight = Math.min(baseline - settings.top, settings.height - settings.bottom - baseline);
    var maxValue = Math.max.apply(null, days.map(function (day) {
      return Math.max(day.additions, day.removals);
    }));
    var scaleMax = Math.max(1000, Math.ceil(maxValue / 1000) * 1000);
    var plotWidth = chartSize.width - settings.left - settings.right;
    var slot = plotWidth / days.length;
    var barWidth = Math.max(2, Math.min(slot * 0.68, 19));
    var bars = "";

    days.forEach(function (day, index) {
      var x = settings.left + index * slot + (slot - barWidth) / 2;
      var additionHeight = (day.additions / scaleMax) * availableHeight;
      var removalHeight = (day.removals / scaleMax) * availableHeight;
      bars += '<rect class="addition-bar" x="' + x + '" y="' + (baseline - additionHeight) + '" width="' + barWidth + '" height="' + additionHeight + '" rx="1"></rect>';
      bars += '<rect class="removal-bar" x="' + x + '" y="' + baseline + '" width="' + barWidth + '" height="' + removalHeight + '" rx="1"></rect>';
    });

    svg.innerHTML =
      '<line class="grid-line" x1="' + settings.left + '" x2="' + (chartSize.width - settings.right) + '" y1="' + settings.top + '" y2="' + settings.top + '"></line>' +
      '<line class="grid-line" x1="' + settings.left + '" x2="' + (chartSize.width - settings.right) + '" y1="' + (settings.height - settings.bottom) + '" y2="' + (settings.height - settings.bottom) + '"></line>' +
      '<line class="baseline" x1="' + settings.left + '" x2="' + (chartSize.width - settings.right) + '" y1="' + baseline + '" y2="' + baseline + '"></line>' +
      '<text class="axis-label" x="' + (settings.left - 11) + '" y="' + (settings.top + 7) + '" text-anchor="end">' + comma(scaleMax) + "</text>" +
      '<text class="axis-label" x="' + (settings.left - 11) + '" y="' + (settings.height - settings.bottom + 7) + '" text-anchor="end">-' + comma(scaleMax) + "</text>" +
      bars +
      chartDateTicks(days, settings.left, settings.right, settings.height - 8);

    attachTooltip(svg, frame, tooltip, days, settings.left, settings.right, function (day) {
      var net = day.additions - day.removals;
      return '<span>' + escapeHtml(fullDate(day)) + '</span><strong>' + signedNumber(net) + ' net coins</strong><span>+' + comma(day.additions) + ' additions</span><br><span>-' + comma(day.removals) + ' removals</span>';
    }, function () {
      return baseline;
    });
  }

  function attachTooltip(svg, frame, tooltip, days, left, right, render, yForDay) {
    function hide() {
      tooltip.classList.remove("is-visible");
      tooltip.setAttribute("aria-hidden", "true");
    }

    function show(event) {
      var bounds = svg.getBoundingClientRect();
      if (!bounds.width) {
        return;
      }
      var viewX = ((event.clientX - bounds.left) / bounds.width) * chartSize.width;
      var plotWidth = chartSize.width - left - right;
      var rawIndex = ((viewX - left) / plotWidth) * Math.max(days.length - 1, 1);
      var index = Math.max(0, Math.min(days.length - 1, Math.round(rawIndex)));
      var day = days[index];
      var frameBounds = frame.getBoundingClientRect();
      var pointerX = event.clientX - frameBounds.left;
      var pointY = (yForDay(day) / (svg.viewBox.baseVal.height || 1)) * frameBounds.height;
      tooltip.style.left = Math.max(86, Math.min(frameBounds.width - 86, pointerX)) + "px";
      tooltip.style.top = Math.max(48, pointY) + "px";
      tooltip.innerHTML = render(day);
      tooltip.classList.add("is-visible");
      tooltip.setAttribute("aria-hidden", "false");
    }

    svg.onpointermove = show;
    svg.onpointerleave = hide;
    svg.onpointerdown = show;
    svg.onkeydown = function (event) {
      if (event.key === "Escape") {
        hide();
      }
    };
  }

  function renderEmptyChart(svg, message, height) {
    svg.innerHTML = '<text class="axis-label" x="500" y="' + Math.round(height / 2) + '" text-anchor="middle">' + escapeHtml(message) + "</text>";
  }

  function sum(days, key) {
    return days.reduce(function (total, day) {
      return total + Number(day[key] || 0);
    }, 0);
  }

  function renderMetrics(sizeDays, activityDays) {
    var latest = sizeDays[sizeDays.length - 1];
    var first = sizeDays[0];
    var totalGrowth = latest ? latest.db_bytes - first.db_bytes : 0;
    var intervals = Math.max(sizeDays.length - 1, 1);
    var dailyGrowth = totalGrowth / intervals;
    var additions = sum(activityDays, "additions");
    var removals = sum(activityDays, "removals");
    var net = additions - removals;
    var rangeName = humanRange();

    setText("latest-size", latest ? bytes(latest.db_bytes) : "--");
    setText("latest-date", latest ? fullDate(latest) + " UTC" : "No observed size");
    setText("growth-label", rangeName + " growth");
    setText("growth-value", latest ? signedBytes(totalGrowth) : "--");
    setText("growth-detail", latest ? bytes(first.db_bytes) + " to " + bytes(latest.db_bytes) : "No observed size");
    setText("daily-growth", latest ? signedBytes(dailyGrowth) : "--");
    setText("latest-height", latest ? comma(latest.peak_height) : "--");
    setText("additions-total", comma(additions));
    setText("removals-total", comma(removals));
    setText("net-total", signedNumber(net));
    setText("footprint-summary", latest ? fullDate(first) + " to " + fullDate(latest) + " / " + sizeDays.length + " daily snapshots" : "No database-size samples in this range.");
  }

  function hasSize(day) {
    return day.has_size !== false;
  }

  function hasActivity(day) {
    return day.has_activity !== false;
  }

  function render() {
    var days = rangeDays();
    var sizeDays = days.filter(hasSize);
    var activityDays = days.filter(hasActivity);
    state.visibleDays = days;
    updateRangeButtons();
    renderMetrics(sizeDays, activityDays);
    renderFootprint(sizeDays);
    renderActivity(activityDays);
  }

  function setDatasetMetadata(payload) {
    var network = String(payload.network || "mainnet").toUpperCase();
    var generated = payload.generated_at ? new Date(payload.generated_at) : null;
    var updated = generated && !Number.isNaN(generated.getTime())
      ? new Intl.DateTimeFormat("en-US", {
        month: "short",
        day: "numeric",
        year: "numeric",
        timeZone: "UTC"
      }).format(generated)
      : "local fixture";
    setText("dataset-meta", network + " / " + state.allDays.length + " daily points");
    setText("footer-updated", "Fixture generated " + updated);
    setText("mode-label", payload.mode === "demo" ? "DEMO MODE" : "STATIC DATA");
  }

  function bindRangeControls() {
    document.querySelectorAll("[data-range]").forEach(function (button) {
      button.addEventListener("click", function () {
        state.range = button.getAttribute("data-range");
        render();
      });
    });
  }

  function showLoadError() {
    var notice = byId("load-error");
    if (notice) {
      notice.hidden = false;
      notice.textContent = "Could not load the local data file. Start a local web server from the project root, then open the printed URL.";
    }
    setText("footprint-summary", "The data fixture is unavailable.");
  }

  function readUint64(view, offset) {
    if (typeof view.getBigUint64 === "function") {
      return Number(view.getBigUint64(offset, true));
    }
    return view.getUint32(offset, true) + view.getUint32(offset + 4, true) * 4294967296;
  }

  function dateFromEpochDay(day) {
    return new Date(day * 86400000).toISOString().slice(0, 10);
  }

  function decodeCdb(buffer, manifest) {
    var headerBytes = 16;
    var expectedRecordBytes = 40;
    if (buffer.byteLength < headerBytes) {
      throw new Error("CDB1 payload is too short");
    }
    var view = new DataView(buffer);
    var magic = String.fromCharCode(view.getUint8(0), view.getUint8(1), view.getUint8(2), view.getUint8(3));
    var version = view.getUint8(4);
    var recordBytes = view.getUint8(5);
    var count = view.getUint32(8, true);
    var startDay = view.getInt32(12, true);
    if (magic !== "CDB1" || version !== 1 || recordBytes !== expectedRecordBytes) {
      throw new Error("Unsupported CDB1 payload");
    }
    if (buffer.byteLength !== headerBytes + count * recordBytes) {
      throw new Error("CDB1 payload length does not match its header");
    }

    var days = [];
    for (var index = 0; index < count; index += 1) {
      var offset = headerBytes + index * recordBytes;
      var flags = view.getUint32(offset + 32, true);
      days.push({
        date: dateFromEpochDay(startDay + index),
        db_bytes: readUint64(view, offset),
        wal_bytes: readUint64(view, offset + 8),
        peak_height: view.getUint32(offset + 16, true),
        blocks: view.getUint32(offset + 20, true),
        additions: view.getUint32(offset + 24, true),
        removals: view.getUint32(offset + 28, true),
        flags: flags,
        has_size: Boolean(flags & 1),
        has_activity: Boolean(flags & 2)
      });
    }
    return Object.assign({}, manifest, { days: days });
  }

  function fetchJson(url) {
    return fetch(url).then(function (response) {
      if (!response.ok) {
        throw new Error("Data request failed");
      }
      return response.json();
    });
  }

  function loadCdb(manifest, manifestUrl) {
    if (!manifest || manifest.format !== "chia-db-daily/cdb1" || !manifest.data_file) {
      return Promise.resolve(manifest);
    }
    var binaryUrl = new URL(manifest.data_file, manifestUrl);
    if (manifest.sha256) {
      binaryUrl.searchParams.set("v", manifest.sha256.slice(0, 16));
    }
    return fetch(binaryUrl.toString())
      .then(function (response) {
        if (!response.ok) {
          throw new Error("CDB1 request failed");
        }
        return response.arrayBuffer();
      })
      .then(function (buffer) {
        return decodeCdb(buffer, manifest);
      });
  }

  function acceptPayload(payload) {
    if (!payload || !Array.isArray(payload.days) || payload.days.length < 2) {
      throw new Error("Data schema is missing daily points");
    }
    state.payload = payload;
    state.allDays = payload.days.slice().sort(function (left, right) {
      return left.date.localeCompare(right.date);
    });
    setDatasetMetadata(payload);
    render();
  }

  function loadData() {
    var search = new URLSearchParams(window.location.search);
    var dataPath = search.get("data") || "data/demo.index.json";
    var manifestUrl = new URL(dataPath, window.location.href).toString();
    return fetchJson(manifestUrl)
      .then(function (manifest) {
        return loadCdb(manifest, manifestUrl);
      })
      .then(acceptPayload)
      .catch(function (error) {
        showLoadError(error);
        return undefined;
      });
  }

  bindRangeControls();
  loadData();
}());
