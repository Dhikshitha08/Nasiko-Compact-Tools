/**
 * Nasiko Tool-Compact — Interactive Simulator Engine
 * Implements Design A schema encoding, validation, fail-closed decoding,
 * and chunk-by-chunk stream simulation.
 */

// ─── Preset Dataset ──────────────────────────────────────────────────────────

const PRESETS = {
  'ct-001': {
    name: 'Single Tool (Calendar Event)',
    tools: [
      {
        type: 'function',
        function: {
          name: 'create_calendar_event',
          description: "Create an event in the user's calendar.",
          parameters: {
            type: 'object',
            properties: {
              title: { type: 'string', description: 'Event title' },
              start: { type: 'string', format: 'date-time', description: 'Start time, ISO 8601' },
              duration_min: { type: 'integer', description: 'Duration in minutes' },
              attendees: { type: 'array', items: { type: 'string' }, description: 'Attendee emails' },
              visibility: { type: 'string', enum: ['public', 'private'] }
            },
            required: ['title', 'start']
          }
        }
      },
      {
        type: 'function',
        function: {
          name: 'send_email',
          description: "Send an email from the user's account.",
          parameters: {
            type: 'object',
            properties: {
              to: { type: 'array', items: { type: 'string' }, description: 'Recipient emails' },
              subject: { type: 'string', description: 'Subject line' },
              body: { type: 'string', description: 'Body text' },
              cc: { type: 'array', items: { type: 'string' } }
            },
            required: ['to', 'subject', 'body']
          }
        }
      }
    ],
    callText: '<<call create_calendar_event {"title":"Design review","start":"2026-10-05T15:00:00+05:30","attendees":["riya@example.com"]}>>',
    chunks: [
      '<<call create_calendar_event {"title":"Design review","start":"2026-10-05T15:00:00+05:30","attendees":["riya@example.com"]}>>'
    ]
  },

  'ct-002': {
    name: 'Multi-Tool (Email + Retro)',
    tools: [
      {
        type: 'function',
        function: {
          name: 'send_email',
          description: "Send an email from the user's account.",
          parameters: {
            type: 'object',
            properties: {
              to: { type: 'array', items: { type: 'string' }, description: 'Recipient emails' },
              subject: { type: 'string', description: 'Subject line' },
              body: { type: 'string', description: 'Body text' },
              cc: { type: 'array', items: { type: 'string' } }
            },
            required: ['to', 'subject', 'body']
          }
        }
      },
      {
        type: 'function',
        function: {
          name: 'create_calendar_event',
          description: "Create an event in the user's calendar.",
          parameters: {
            type: 'object',
            properties: {
              title: { type: 'string', description: 'Event title' },
              start: { type: 'string', format: 'date-time', description: 'Start time, ISO 8601' },
              duration_min: { type: 'integer', description: 'Duration in minutes' },
              attendees: { type: 'array', items: { type: 'string' }, description: 'Attendee emails' },
              visibility: { type: 'string', enum: ['public', 'private'] }
            },
            required: ['title', 'start']
          }
        }
      }
    ],
    callText: 'I have dispatched the status update email and placed the retrospective on your schedule:\n<<call send_email {"to":["sam@example.com"],"subject":"Build status","body":"The build is green."}>>\n<<call create_calendar_event {"title":"Retro","start":"2026-10-04T10:00:00+05:30","duration_min":30,"visibility":"private"}>>',
    chunks: [
      'I have dispatched the status update email:\n',
      '<<call send_email {"to":["sam@example.com"],"subject":"Build status","body":"The build is green."}>>\n',
      '<<call create_calendar_event {"title":"Retro","start":"2026-10-04T10:00:00+05:30","duration_min":30,"visibility":"private"}>>'
    ]
  },

  'ct-003': {
    name: 'Plain Text (No Calls)',
    tools: [
      {
        type: 'function',
        function: {
          name: 'create_calendar_event',
          description: "Create an event in the user's calendar.",
          parameters: {
            type: 'object',
            properties: {
              title: { type: 'string' },
              start: { type: 'string', format: 'date-time' }
            },
            required: ['title', 'start']
          }
        }
      }
    ],
    callText: 'I am unable to answer weather queries because no weather service tool is currently enabled for this agent session.',
    chunks: [
      'I am unable to answer weather queries because ',
      'no weather service tool is currently enabled.'
    ]
  },

  'dc-002': {
    name: 'Stream Marker Split (<<ca + ll ...>>)',
    tools: [
      {
        type: 'function',
        function: {
          name: 'create_calendar_event',
          description: "Create an event in the user's calendar.",
          parameters: {
            type: 'object',
            properties: {
              title: { type: 'string' },
              start: { type: 'string', format: 'date-time' }
            },
            required: ['title', 'start']
          }
        }
      }
    ],
    callText: '<<call create_calendar_event {"title":"Retro","start":"2026-10-04T10:00:00+05:30"}>>',
    chunks: [
      '<<ca',
      'll create_calendar_event {"title":"Ret',
      'ro","start":"2026-10-04T10:00:00+05:30"}>',
      '>'
    ]
  },

  'dc-004': {
    name: 'Fail-Closed: Unknown Tool',
    tools: [
      {
        type: 'function',
        function: {
          name: 'create_calendar_event',
          description: "Create an event in the user's calendar.",
          parameters: {
            type: 'object',
            properties: {
              title: { type: 'string' },
              start: { type: 'string', format: 'date-time' }
            },
            required: ['title', 'start']
          }
        }
      }
    ],
    callText: '<<call schedule_meeting {"title":"1-on-1"}>>',
    chunks: [
      '<<call schedule_meeting {"title":"1-on-1"}>>'
    ]
  },

  'dc-005': {
    name: 'Fail-Closed: Missing Required & Bad Enum',
    tools: [
      {
        type: 'function',
        function: {
          name: 'create_calendar_event',
          description: "Create an event in the user's calendar.",
          parameters: {
            type: 'object',
            properties: {
              title: { type: 'string' },
              start: { type: 'string', format: 'date-time' },
              visibility: { type: 'string', enum: ['public', 'private'] }
            },
            required: ['title', 'start']
          }
        }
      }
    ],
    callText: '<<call create_calendar_event {"start":"2026-10-05T15:00:00+05:30","visibility":"confidential"}>>',
    chunks: [
      '<<call create_calendar_event {"start":"2026-10-05T15:00:00+05:30","visibility":"confidential"}>>'
    ]
  }
};

// ─── Encoder Engine (Design A) ───────────────────────────────────────────────

function encodeType(schema) {
  if (!schema || typeof schema !== 'object') return 'any';
  
  if (Array.isArray(schema.enum)) {
    return schema.enum.join('|');
  }

  const type = schema.type;
  if (type === 'string') {
    if (schema.format === 'date-time') return 'datetime';
    return 'str';
  } else if (type === 'integer') {
    return 'int';
  } else if (type === 'number') {
    return 'float';
  } else if (type === 'boolean') {
    return 'bool';
  } else if (type === 'array') {
    const itemType = schema.items ? encodeType(schema.items) : 'any';
    return `[${itemType}]`;
  } else if (type === 'object') {
    return 'obj';
  }
  return 'any';
}

function encodeSingleTool(tool) {
  const fn = tool.function || tool;
  const name = fn.name;
  const desc = fn.description || '';
  const params = fn.parameters || {};
  const props = params.properties || {};
  const required = new Set(params.required || []);

  const argParts = [];
  for (const [propName, propSchema] of Object.entries(props)) {
    const isRequired = required.has(propName);
    const typeStr = encodeType(propSchema);
    const optMarker = isRequired ? '' : '?';
    argParts.push(`${propName}${optMarker}:${typeStr}`);
  }

  const signature = `${name}(${argParts.join(', ')})`;
  if (desc) {
    return `${signature} - ${desc}`;
  }
  return signature;
}

function encodeTools(toolsList) {
  const lines = toolsList.map(t => encodeSingleTool(t));
  return `Tools:\n${lines.join('\n')}\n\nTo call a tool, emit: <<call name {json args}>>`;
}

// ─── Decoder Engine ──────────────────────────────────────────────────────────

function decodeCalls(rawText, toolsList) {
  const toolMap = new Map();
  toolsList.forEach(t => {
    const fn = t.function || t;
    toolMap.set(fn.name, t);
  });

  const calls = [];
  const markerRegex = /<<call\s+([a-zA-Z0-9_\-]+)\s+/g;
  let match;

  while ((match = markerRegex.exec(rawText)) !== null) {
    const toolName = match[1];
    const afterNameIndex = match.index + match[0].length;
    
    // Check if tool is known
    const toolDef = toolMap.get(toolName);
    if (!toolDef) {
      throw { slug: 'unknown_tool', message: `Unknown tool: '${toolName}'` };
    }

    // Scan balanced JSON braces
    let depth = 0;
    let inString = false;
    let escape = false;
    let jsonEnd = -1;

    for (let i = afterNameIndex; i < rawText.length; i++) {
      const ch = rawText[i];
      if (escape) {
        escape = false;
        continue;
      }
      if (ch === '\\') {
        escape = true;
        continue;
      }
      if (ch === '"') {
        inString = !inString;
        continue;
      }
      if (!inString) {
        if (ch === '{') depth++;
        else if (ch === '}') {
          depth--;
          if (depth === 0) {
            jsonEnd = i + 1;
            break;
          }
        }
      }
    }

    if (jsonEnd === -1) {
      throw { slug: 'invalid_arguments', message: 'Malformed JSON argument object' };
    }

    const jsonSnippet = rawText.slice(afterNameIndex, jsonEnd).trim();
    let parsedArgs;
    try {
      parsedArgs = JSON.parse(jsonSnippet);
    } catch (e) {
      throw { slug: 'invalid_arguments', message: `JSON syntax error: ${e.message}` };
    }

    // Validate parameters against schema
    const fn = toolDef.function || toolDef;
    const params = fn.parameters || {};
    const props = params.properties || {};
    const required = params.required || [];

    for (const reqField of required) {
      if (!(reqField in parsedArgs) || parsedArgs[reqField] === null || parsedArgs[reqField] === undefined) {
        throw { slug: 'invalid_arguments', message: `Missing required field: '${reqField}'` };
      }
    }

    for (const [k, v] of Object.entries(parsedArgs)) {
      const propDef = props[k];
      if (propDef && propDef.enum) {
        if (!propDef.enum.includes(v)) {
          throw { slug: 'invalid_arguments', message: `Field '${k}' violates enum: got '${v}', allowed: [${propDef.enum.join(', ')}]` };
        }
      }
    }

    calls.push({
      id: `call_${calls.length + 1}`,
      type: 'function',
      function: {
        name: toolName,
        arguments: JSON.stringify(parsedArgs)
      }
    });

    markerRegex.lastIndex = jsonEnd;
  }

  return calls;
}

// ─── Approximate Token Counter ───────────────────────────────────────────────

function countTokens(text) {
  if (!text) return 0;
  // Standard GPT-4o approximation: ~3.8 chars per token for code/JSON schemas
  const words = text.trim().split(/\s+/).filter(Boolean).length;
  const chars = text.length;
  return Math.max(1, Math.round((chars / 4.0 + words) / 2));
}

// ─── UI Controller ───────────────────────────────────────────────────────────

let currentPresetKey = 'ct-001';
let currentTools = [];

const dom = {
  presetButtons: document.querySelectorAll('.scenario-chip, .preset-btn'),
  baselineTokenDisplay: document.getElementById('baselineTokenDisplay'),
  compactTokenDisplay: document.getElementById('compactTokenDisplay'),
  savingsPctDisplay: document.getElementById('savingsPctDisplay'),
  progressCircle: document.getElementById('progressCircle'),
  tokenSavingsTotal: document.getElementById('tokenSavingsTotal'),
  costSavingsDisplay: document.getElementById('costSavingsDisplay'),
  schemaInput: document.getElementById('schemaInput'),
  compactOutput: document.getElementById('compactOutput'),
  schemaTokenTag: document.getElementById('schemaTokenTag'),
  compactTokenTag: document.getElementById('compactTokenTag'),
  reEncodeBtn: document.getElementById('reEncodeBtn'),
  callInput: document.getElementById('callInput'),
  decodedOutput: document.getElementById('decodedOutput'),
  callTokenTag: document.getElementById('callTokenTag'),
  streamSimBtn: document.getElementById('streamSimBtn'),
  decodeNowBtn: document.getElementById('decodeNowBtn'),
  streamVisualizer: document.getElementById('streamVisualizer'),
  streamStatus: document.getElementById('streamStatus'),
  chunkChips: document.getElementById('chunkChips'),
  resultStatusBadge: document.getElementById('resultStatusBadge'),
  decoderStatusTag: document.getElementById('decoderStatusTag')
};

function loadPreset(key) {
  const preset = PRESETS[key];
  if (!preset) return;
  currentPresetKey = key;
  currentTools = preset.tools;

  // Update preset buttons active state
  dom.presetButtons.forEach(btn => {
    btn.classList.toggle('active', btn.dataset.preset === key);
  });

  // Populate Schema Input
  dom.schemaInput.value = JSON.stringify(preset.tools, null, 2);
  
  // Populate Call Input
  dom.callInput.value = preset.callText;
  
  // Hide stream visualizer
  dom.streamVisualizer.style.display = 'none';

  // Run Encode & Decode
  updatePipeline();
}

function updatePipeline() {
  let tools;
  try {
    tools = JSON.parse(dom.schemaInput.value);
    currentTools = Array.isArray(tools) ? tools : [tools];
  } catch (err) {
    dom.compactOutput.textContent = `// Error parsing tools JSON:\n${err.message}`;
    return;
  }

  // 1. Encode Tools
  const compactText = encodeTools(currentTools);
  dom.compactOutput.textContent = compactText;

  // 2. Measure Tokens
  const rawSchemaString = JSON.stringify(currentTools);
  const baselineTokens = countTokens(rawSchemaString);
  const compactTokens = countTokens(compactText);
  const savingsPct = Math.max(0, Math.round(((baselineTokens - compactTokens) / baselineTokens) * 100));

  dom.baselineTokenDisplay.textContent = baselineTokens;
  dom.compactTokenDisplay.textContent = compactTokens;
  dom.savingsPctDisplay.textContent = `-${savingsPct}%`;
  
  // Animate circular progress ring (circ = 113.1 for r=18)
  const offset = 113.1 - (113.1 * (savingsPct / 100));
  dom.progressCircle.style.strokeDashoffset = offset;

  dom.schemaTokenTag.textContent = `Tokens: ~${baselineTokens}`;
  dom.compactTokenTag.textContent = `Tokens: ~${compactTokens} (-${savingsPct}%)`;

  const per100k = ((baselineTokens - compactTokens) * 100000 / 1000000).toFixed(1);
  dom.tokenSavingsTotal.textContent = `${per100k}M tokens`;
  const dollarSaved = (((baselineTokens - compactTokens) * 100000 / 1000000) * 5.0).toFixed(2);
  dom.costSavingsDisplay.textContent = `~$${dollarSaved} saved per 100k calls`;

  // 3. Decode Calls
  runDecode();
}

function runDecode() {
  const callText = dom.callInput.value;
  try {
    const calls = decodeCalls(callText, currentTools);
    const standardOutput = {
      tool_calls: calls
    };
    dom.decodedOutput.textContent = JSON.stringify(standardOutput, null, 2);
    dom.decodedOutput.classList.remove('error-text');
    
    dom.resultStatusBadge.className = 'status-indicator-badge success';
    dom.resultStatusBadge.innerHTML = `<span class="status-dot"></span> Validated (${calls.length} calls)`;
    dom.decoderStatusTag.className = 'tag tag-indigo';
    dom.decoderStatusTag.textContent = 'OpenAI ToolCall';
  } catch (err) {
    const errorOutput = {
      error: err.slug || 'decode_error',
      message: err.message,
      policy: 'FAIL_CLOSED (Call not guessed or partially executed)'
    };
    dom.decodedOutput.textContent = JSON.stringify(errorOutput, null, 2);
    dom.decodedOutput.classList.add('error-text');
    
    dom.resultStatusBadge.className = 'status-indicator-badge error';
    dom.resultStatusBadge.innerHTML = `<span class="status-dot"></span> Fail-Closed (${err.slug})`;
    dom.decoderStatusTag.className = 'tag tag-purple';
    dom.decoderStatusTag.textContent = 'Error: ' + err.slug;
  }
}

// ─── Stream Simulation ───────────────────────────────────────────────────────

let isSimulating = false;

async function simulateStream() {
  if (isSimulating) return;
  isSimulating = true;
  dom.streamSimBtn.disabled = true;

  const preset = PRESETS[currentPresetKey];
  const chunks = preset.chunks || [dom.callInput.value];

  dom.streamVisualizer.style.display = 'block';
  dom.chunkChips.innerHTML = '';
  
  chunks.forEach((chunk, i) => {
    const chip = document.createElement('span');
    chip.className = 'chunk-chip';
    chip.id = `chunk-chip-${i}`;
    chip.textContent = `Chunk ${i + 1}: ${JSON.stringify(chunk)}`;
    dom.chunkChips.appendChild(chip);
  });

  dom.callInput.value = '';
  let accumulated = '';

  for (let i = 0; i < chunks.length; i++) {
    const chip = document.getElementById(`chunk-chip-${i}`);
    if (chip) chip.classList.add('active');
    
    dom.streamStatus.textContent = `Receiving Chunk ${i + 1}/${chunks.length} (Length: ${chunks[i].length} bytes)...`;
    accumulated += chunks[i];
    dom.callInput.value = accumulated;
    
    // Check if complete call marker exists before updating decoded output
    const hasCompleteCall = accumulated.includes('>>') && accumulated.includes('<<call');
    if (hasCompleteCall) {
      runDecode();
    } else {
      dom.decodedOutput.textContent = '// Accumulating stream buffer... (waiting for complete marker ">>")';
      dom.resultStatusBadge.className = 'status-indicator-badge';
      dom.resultStatusBadge.innerHTML = `<span class="status-dot"></span> Incomplete Buffer (${accumulated.length} chars)`;
    }

    await new Promise(r => setTimeout(r, 650));
    if (chip) chip.classList.remove('active');
  }

  dom.streamStatus.textContent = 'Stream complete. Final buffer validated.';
  runDecode();
  isSimulating = false;
  dom.streamSimBtn.disabled = false;
}

// ─── Event Listeners ─────────────────────────────────────────────────────────

dom.presetButtons.forEach(btn => {
  btn.addEventListener('click', () => loadPreset(btn.dataset.preset));
});

dom.reEncodeBtn.addEventListener('click', updatePipeline);
dom.decodeNowBtn.addEventListener('click', runDecode);
dom.streamSimBtn.addEventListener('click', simulateStream);

dom.schemaInput.addEventListener('input', () => {
  // auto re-calculate
  clearTimeout(dom.schemaInput._t);
  dom.schemaInput._t = setTimeout(updatePipeline, 300);
});

dom.callInput.addEventListener('input', () => {
  clearTimeout(dom.callInput._t);
  dom.callInput._t = setTimeout(runDecode, 300);
});

// Initial boot
loadPreset('ct-001');
