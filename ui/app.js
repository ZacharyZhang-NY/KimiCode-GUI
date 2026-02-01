(() => {
  const tauri = window.__TAURI__ || null;
  const invoke = tauri?.core?.invoke;
  const listen = tauri?.event?.listen;

  const state = {
    settings: {
      work_dir: null,
      config_file: null,
      mcp_config_files: [],
      skills_dir: null,
      model: null,
      thinking: false,
      yolo: false,
      pinned_sessions: [],
    },
    paths: null,
    config: null,
    mcp: null,
    skills: [],
    sessions: [],
    currentSession: null,
    messages: [],
    isStreaming: false,
    attachedFiles: [],
    currentStreamId: null,
    isLoggedIn: false,
    models: [],
    user: null,
    isEditingSessions: false,
    // File explorer state
    fileTree: null,
    expandedFolders: new Set(),
    openTabs: [],
    activeTab: null,
    explorerCollapsed: false,
    gitStatus: {},
    fileModified: {},
  };
  
  // Tab types
  const TAB_TYPES = {
    FILE: 'file',
    SESSION: 'session'
  };
  
  // Autocomplete state
  const autocomplete = {
    active: false,
    type: null, // 'slash', 'skill', 'file'
    query: '',
    suggestions: [],
    selectedIndex: 0,
    targetInput: null,
    triggerStart: 0,
  };
  
  // Slash commands definition
  const slashCommands = [
    { name: 'help', description: 'Show help information', aliases: ['h', '?'] },
    { name: 'clear', description: 'Clear the context', aliases: ['reset'] },
    { name: 'compact', description: 'Compact the context' },
    { name: 'skill', description: 'Use a skill (e.g., /skill:name)', aliases: [] },
  ];

  const $ = (id) => document.getElementById(id);
  const $$ = (sel, ctx = document) => ctx.querySelectorAll(sel);

  let elements = {};

  function cacheElements() {
    elements = {
      btnNewSession: $('btn-new-session'),
      sessionList: $('session-list'),
      btnEditSessions: $('btn-edit-sessions'),
      userBar: $('user-bar'),
      userStatus: $('user-status'),
      emptyState: $('empty-state'),
      promptInput: $('prompt-input'),
      btnSend: $('btn-send'),
      btnConfig: $('btn-config'),
      btnConfigChat: $('btn-config-chat'),
      yoloSwitchMain: $('yolo-switch-main'),
      yoloSwitchChat: $('yolo-switch-chat'),
      btnFolder: $('btn-folder'),
      btnModel: $('btn-model'),
      folderLabel: $('folder-label'),
      modelLabel: $('model-label'),
      chatView: $('chat-view'),
      chatTitle: $('chat-title'),
      btnCloseChat: $('btn-close-chat'),
      messages: $('messages'),
      chatInput: $('chat-input'),
      btnChatSend: $('btn-chat-send'),
      drawerBackdrop: $('drawer-backdrop'),
      btnCloseSettings: $('btn-close-settings'),
      drawerTabs: $$('.drawer-tab'),
      settingWorkdir: $('setting-workdir'),
      settingConfig: $('setting-config'),
      settingMcp: $('setting-mcp'),
      settingSkills: $('setting-skills'),
      settingDefaultModel: $('setting-default-model'),
      settingThinking: $('setting-thinking'),
      settingYolo: $('setting-yolo'),
      settingApiKey: $('setting-api-key'),
      settingApiBase: $('setting-api-base'),
      btnSaveSettings: $('btn-save-settings'),
      modelList: $('model-list'),
      skillsList: $('skills-list'),
      mcpEditor: $('mcp-editor'),
      configEditor: $('config-editor'),
      btnSaveMcp: $('btn-save-mcp'),
      btnSaveConfig: $('btn-save-config'),
      // Login modal elements
      loginModal: $('login-modal'),
      btnCloseLogin: $('btn-close-login'),
      btnLoginStart: $('btn-login-start'),
      btnCancelLogin: $('btn-cancel-login'),
      btnOpenBrowser: $('btn-open-browser'),
      loginContent: $('login-content'),
      loginProgress: $('login-progress'),
      loginUserCode: $('login-user-code'),
      loginStatus: $('login-status'),
      // New login method elements
      loginMethods: $('login-methods'),
      loginOptionOauth: $('login-option-oauth'),
      loginOptionApikey: $('login-option-apikey'),
      loginOauthFlow: $('login-oauth-flow'),
      loginApikeyForm: $('login-apikey-form'),
      oauthStart: $('oauth-start'),
      oauthProgress: $('oauth-progress'),
      btnBackToMethods: $('btn-back-to-methods'),
      btnBackFromApikey: $('btn-back-from-apikey'),
      apiKeyInput: $('api-key-input'),
      apiBaseInput: $('api-base-input'),
      btnSaveApikey: $('btn-save-apikey'),
      folderModal: $('folder-modal'),
      btnCloseFolder: $('btn-close-folder'),
      folderList: $('folder-list'),
      customFolderInput: $('custom-folder-input'),
      btnAddCustomFolder: $('btn-add-custom-folder'),
      modelModal: $('model-modal'),
      btnCloseModel: $('btn-close-model'),
      modelOptions: $('model-options'),
      loadingIndicator: $('loading-indicator'),
      toolApprovalModal: $('tool-approval-modal'),
      btnCloseToolApproval: $('btn-close-tool-approval'),
      toolApprovalTitle: $('tool-approval-title'),
      toolApprovalDetails: $('tool-approval-details'),
      btnToolApprove: $('btn-tool-approve'),
      btnToolReject: $('btn-tool-reject'),
      // File explorer elements
      fileExplorer: $('file-explorer'),
      fileExplorerContent: $('file-explorer-content'),
      btnRefreshFiles: $('btn-refresh-files'),
      btnCollapseExplorer: $('btn-collapse-explorer'),
      btnToggleExplorer: $('btn-toggle-explorer'),
      appContainer: document.querySelector('.app'),
      tabBar: $('tab-bar'),
      tabList: $('tab-list'),
      fileView: $('file-view'),
      fileViewContent: $('file-view-content'),
      btnEditFile: $('btn-edit-file'),
      btnSaveFile: $('btn-save-file'),
      btnCancelEdit: $('btn-cancel-edit'),
      fileViewEditor: $('file-view-editor'),
      fileEditorTextarea: $('file-editor-textarea'),
      fileViewHeader: document.querySelector('.file-view-header'),
    };
  }

  async function init() {
    if (!invoke) {
      showError('Tauri API not available. Please restart the application.');
      return;
    }

    cacheElements();

    try {
      const info = await invoke('app_info');
      document.title = `Kimi ${info.version}`;

      state.paths = await invoke('app_paths');
      
      const payload = await invoke('gui_settings_load', { path: null });
      state.settings = { ...state.settings, ...payload.settings };
      
      await checkAuthStatus();
      await loadConfig();
      await loadMcp();
      await loadSkills();
      
      // Set initial logged-in class
      if (elements.appContainer) {
        elements.appContainer.classList.toggle('logged-in', state.isLoggedIn);
      }
      
      // Load file tree if work_dir is set and user is logged in
      if (state.isLoggedIn && state.settings.work_dir) {
        await loadFileTree();
      }
      
      // Only load sessions and models if logged in
      if (state.isLoggedIn) {
        await loadSessions();
        await loadModels();
      } else {
        // Show login prompt instead of sessions
        elements.sessionList.innerHTML = '<div style="padding: 24px 16px; text-align: center; color: var(--text-muted); font-size: 13px;">Please login to view sessions</div>';
        // Show login prompt in main area
        showLoginPrompt();
      }
      
      // Set initial toggle button state
      if (elements.btnToggleExplorer) {
        elements.btnToggleExplorer.classList.toggle('hidden', !state.explorerCollapsed);
      }
      
      initEvents();
      updateUI();
      setupMarked();
      
      if (listen) {
        listen('chat://event', handleChatEvent);
        listen('oauth://event', handleOAuthEvent);
      }
    } catch (err) {
      const message = err?.message || err || 'Initialization failed';
      showError(`Initialization failed: ${message}`);
    }
  }

  function setupMarked() {
    if (typeof marked === 'undefined') return;
    
    marked.setOptions({
      breaks: true,
      gfm: true,
      headerIds: false,
      mangle: false,
      sanitize: false,
      smartLists: true,
      smartypants: true,
      xhtml: false,
      highlight: (code, lang) => {
        if (typeof hljs !== 'undefined' && lang) {
          try {
            return hljs.highlight(code, { language: lang }).value;
          } catch (e) {
            return code;
          }
        }
        return code;
      }
    });
  }

  function handleChatEvent(event) {
    const { event: eventType, data } = event.payload;
    
    switch (eventType) {
      case 'chunk':
        if (data?.content) {
          appendStreamingText(data.content);
        }
        break;
      case 'thinking':
        if (data?.content) {
          appendThinkingText(data.content);
        }
        break;
      case 'done':
        // Track token usage if available
        if (data?.usage && state.authMode === 'api_key') {
          trackTokenUsage(data.usage);
        }
        finishStreaming();
        break;
      case 'cancelled':
        finishStreaming();
        break;
      case 'tool_status':
        handleToolStatus(data);
        break;
      case 'tool_result':
        handleToolResult(data);
        break;
      case 'tool_approval':
        openToolApprovalModal(data);
        break;
      case 'error':
        showError(data?.message || 'An error occurred');
        finishStreaming();
        break;
    }
  }

  function handleOAuthEvent(event) {
    const { event: eventType, data } = event.payload;
    
    switch (eventType) {
      case 'waiting':
        if (elements.loginStatus) {
          elements.loginStatus.textContent = data.message || 'Waiting for authorization...';
        }
        break;
        
      case 'success':
        state.isLoggedIn = true;
        closeLoginModal();
        updateUserBar();
        loadSessions();
        loadModels();
        loadUserProfile();
        showSuccess('Login successful!');
        // Refresh the main view to show the actual interface
        location.reload();
        break;
        
      case 'error':
        showError(data.message || 'Login failed');
        resetLoginModal();
        break;
    }
  }

  let currentMessageEl = null;
  let currentTextBuffer = '';
  let currentThinkingEl = null;
  let currentThinkingBuffer = '';
  const toolMessages = new Map();
  let pendingApprovalId = null;

  function appendStreamingText(text) {
    if (!currentMessageEl) {
      currentMessageEl = createMessageElement('assistant', '');
      currentMessageEl.classList.add('streaming');
      elements.messages.appendChild(currentMessageEl);
      scrollToBottom();
    }
    
    currentTextBuffer += text;
    
    // Capture reference to avoid race condition with finishStreaming
    const msgEl = currentMessageEl;
    const buffer = currentTextBuffer;
    
    requestAnimationFrame(() => {
      if (!msgEl.isConnected) return; // Element was removed
      const body = msgEl.querySelector('.message-body');
      try {
        body.innerHTML = marked.parse(buffer);
        if (typeof hljs !== 'undefined') {
          body.querySelectorAll('pre code').forEach((block) => {
            hljs.highlightElement(block);
          });
        }
        scrollToBottom();
      } catch (e) {
        body.textContent = buffer;
      }
    });
  }
  
  function appendThinkingText(text) {
    if (!currentThinkingEl) {
      // Create thinking container before the main message
      currentThinkingEl = document.createElement('div');
      currentThinkingEl.className = 'message thinking';
      currentThinkingEl.innerHTML = `
        <div class="message-avatar thinking">K</div>
        <div class="message-content">
          <div class="message-header">
            <span class="message-author">Thinking</span>
            <span class="message-time">${new Date().toLocaleTimeString()}</span>
          </div>
          <div class="message-body thinking-body"></div>
        </div>
      `;
      elements.messages.appendChild(currentThinkingEl);
      scrollToBottom();
    }
    
    currentThinkingBuffer += text;
    
    // Capture reference to avoid race condition
    const thinkingEl = currentThinkingEl;
    const buffer = currentThinkingBuffer;
    
    requestAnimationFrame(() => {
      if (!thinkingEl.isConnected) return;
      const body = thinkingEl.querySelector('.message-body');
      body.textContent = buffer;
      scrollToBottom();
    });
  }

  async function finishStreaming() {
    // Save assistant message before clearing
    if (state.currentSession && currentTextBuffer.trim()) {
      try {
        await invoke('session_save_message', {
          sessionId: state.currentSession.id,
          role: 'assistant',
          content: currentTextBuffer
        });
      } catch (err) {
        const message = err?.message || err || 'Failed to save assistant message';
        showError(message);
      }
    }
    
    state.isStreaming = false;
    state.currentStreamId = null;
    
    if (currentMessageEl) {
      currentMessageEl.classList.remove('streaming');
      currentMessageEl = null;
    }
    
    if (currentThinkingEl) {
      currentThinkingEl.classList.remove('streaming');
      currentThinkingEl = null;
    }
    
    currentTextBuffer = '';
    currentThinkingBuffer = '';
    hideLoading();
    enableInputs(true);
  }

  // Token usage tracking for API Key mode
  function trackTokenUsage(usage) {
    if (!state.tokenUsage) {
      state.tokenUsage = {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
      };
    }
    state.tokenUsage.prompt_tokens += usage.prompt_tokens || 0;
    state.tokenUsage.completion_tokens += usage.completion_tokens || 0;
    state.tokenUsage.total_tokens += usage.total_tokens || 0;
    
    // Update user bar to show token usage
    if (state.authMode === 'api_key') {
      updateUserBarWithTokens();
    }
  }
  
  function updateUserBarWithTokens() {
    if (!state.isLoggedIn || state.authMode !== 'api_key' || !state.tokenUsage) return;
    
    elements.userStatus.innerHTML = '';
    
    // Token usage row
    const tokenRow = document.createElement('div');
    tokenRow.className = 'quota-row';
    tokenRow.innerHTML = `
      <div class="quota-header">
        <span class="quota-label">Tokens Used</span>
        <span class="quota-reset">Session</span>
      </div>
      <div class="quota-main">
        <span class="quota-percent">${state.tokenUsage.total_tokens.toLocaleString()}</span>
        <div style="font-size: 11px; color: var(--text-muted);">
          ↑${state.tokenUsage.prompt_tokens.toLocaleString()} ↓${state.tokenUsage.completion_tokens.toLocaleString()}
        </div>
      </div>
    `;
    elements.userStatus.appendChild(tokenRow);
    
    const hint = document.createElement('div');
    hint.className = 'user-status-hint';
    hint.textContent = 'Click to logout';
    elements.userStatus.appendChild(hint);
  }

  function createMessageElement(role, content) {
    const div = document.createElement('div');
    div.className = `message ${role}`;
    div.innerHTML = `
      <div class="message-avatar ${role}">${role === 'user' ? 'U' : 'K'}</div>
      <div class="message-content">
        <div class="message-header">
          <span class="message-author">${role === 'user' ? 'You' : 'Kimi'}</span>
          <span class="message-time">${new Date().toLocaleTimeString()}</span>
        </div>
        <div class="message-body">${content ? marked.parse(content) : '<span class="typing-indicator"><span></span><span></span><span></span></span>'}</div>
      </div>
    `;
    return div;
  }

  function createToolMessageElement(label) {
    const div = document.createElement('div');
    div.className = 'message tool';
    div.innerHTML = `
      <div class="message-avatar tool">T</div>
      <div class="message-content">
        <div class="message-header">
          <span class="message-author">Tool</span>
          <span class="message-time">${new Date().toLocaleTimeString()}</span>
        </div>
        <div class="message-body"></div>
      </div>
    `;
    const body = div.querySelector('.message-body');
    body.textContent = label || '';
    return div;
  }

  function handleToolStatus(data) {
    const toolCallId = data?.tool_call_id;
    if (!toolCallId) return;
    let item = toolMessages.get(toolCallId);
    const label = data?.label || data?.name || 'Tool';
    if (!item) {
      item = createToolMessageElement(label);
      toolMessages.set(toolCallId, item);
      elements.messages.appendChild(item);
    }
    const body = item.querySelector('.message-body');
    if (data?.state === 'end') {
      const summary = data?.summary;
      body.textContent = summary ? `${label}\n${summary}` : label;
    } else {
      body.textContent = label;
    }
    scrollToBottom();
  }

  function handleToolResult(data) {
    const toolCallId = data?.tool_call_id;
    if (!toolCallId) return;
    let item = toolMessages.get(toolCallId);
    const label = data?.name ? `Tool ${data.name}` : 'Tool';
    if (!item) {
      item = createToolMessageElement(label);
      toolMessages.set(toolCallId, item);
      elements.messages.appendChild(item);
    }
    const body = item.querySelector('.message-body');
    let text = body.textContent || label;
    if (data?.summary && !text.includes(data.summary)) {
      text = `${text}\n${data.summary}`;
    }
    if (data?.output) {
      text = `${text}\n\n${data.output}`;
    }
    body.textContent = text;
    scrollToBottom();
  }

  function openToolApprovalModal(data) {
    pendingApprovalId = data?.request_id || null;
    if (!pendingApprovalId) return;
    const toolName = data?.name || 'Tool';
    elements.toolApprovalTitle.textContent = `需要批准：${toolName}`;
    elements.toolApprovalDetails.textContent = JSON.stringify(data?.args || {}, null, 2);
    elements.toolApprovalModal.classList.add('open');
  }

  async function respondToolApproval(approved) {
    if (!pendingApprovalId) return;
    try {
      await invoke('tool_approval_respond', {
        requestId: pendingApprovalId,
        approved
      });
    } catch (err) {
      showError(err.message || 'Failed to submit approval');
    }
    pendingApprovalId = null;
    elements.toolApprovalModal.classList.remove('open');
  }

  function scrollToBottom() {
    elements.messages.scrollTop = elements.messages.scrollHeight;
  }

  function showLoading(message = 'Thinking...') {
    if (elements.loadingIndicator) {
      elements.loadingIndicator.textContent = message;
      elements.loadingIndicator.style.display = 'flex';
    }
  }

  function hideLoading() {
    if (elements.loadingIndicator) {
      elements.loadingIndicator.style.display = 'none';
    }
  }

  function showError(message) {
    const errorEl = document.createElement('div');
    errorEl.className = 'error-toast';
    errorEl.textContent = message;
    document.body.appendChild(errorEl);
    
    setTimeout(() => {
      errorEl.classList.add('show');
    }, 10);
    
    setTimeout(() => {
      errorEl.classList.remove('show');
      setTimeout(() => errorEl.remove(), 300);
    }, 5000);
  }

  function showSuccess(message) {
    const toast = document.createElement('div');
    toast.className = 'success-toast';
    toast.textContent = message;
    document.body.appendChild(toast);
    
    setTimeout(() => {
      toast.classList.add('show');
    }, 10);
    
    setTimeout(() => {
      toast.classList.remove('show');
      setTimeout(() => toast.remove(), 300);
    }, 2000);
  }

  function enableInputs(enabled) {
    elements.btnSend.disabled = !enabled;
    elements.btnChatSend.disabled = !enabled;
    elements.promptInput.disabled = !enabled;
    elements.chatInput.disabled = !enabled;
    
    if (enabled) {
      elements.btnSend.classList.remove('disabled');
      elements.btnChatSend.classList.remove('disabled');
    } else {
      elements.btnSend.classList.add('disabled');
      elements.btnChatSend.classList.add('disabled');
    }
  }

  // ================================
  // File Explorer Functions
  // ================================
  
  async function loadFileTree() {
    const workDir = state.settings.work_dir;
    if (!workDir) {
      elements.fileExplorerContent.innerHTML = '<div class="file-explorer-empty">Select a folder to view files</div>';
      return;
    }
    
    try {
      const tree = await invoke('list_dir_tree', { path: workDir });
      state.fileTree = tree;
      
      // Store git status in a lookup map
      state.gitStatus = {};
      if (tree.git_status) {
        tree.git_status.forEach(status => {
          state.gitStatus[status.path] = status.status;
        });
      }
      
      renderFileTree();
    } catch (err) {
      console.error('Failed to load file tree:', err);
      elements.fileExplorerContent.innerHTML = '<div class="file-explorer-empty">Failed to load files</div>';
    }
  }
  
  function getGitStatusForPath(relPath) {
    // Check exact match
    if (state.gitStatus[relPath]) {
      return state.gitStatus[relPath];
    }
    // Check if any parent directory has status
    for (const [path, status] of Object.entries(state.gitStatus)) {
      if (path.startsWith(relPath + '/')) {
        return 'modified'; // Directory contains changes
      }
    }
    return null;
  }
  
  function renderFileTree() {
    if (!state.fileTree) return;
    
    const container = document.createElement('div');
    container.className = 'file-tree';
    
    // Sort entries: folders first, then files
    const sortedEntries = [...state.fileTree.entries].sort((a, b) => {
      if (a.is_dir === b.is_dir) {
        return a.name.localeCompare(b.name);
      }
      return a.is_dir ? -1 : 1;
    });
    
    sortedEntries.forEach(entry => {
      const item = createFileTreeItem(entry, 0);
      container.appendChild(item);
    });
    
    elements.fileExplorerContent.innerHTML = '';
    elements.fileExplorerContent.appendChild(container);
  }
  
  function createFileTreeItem(entry, depth) {
    const hasChildren = entry.is_dir && entry.children && entry.children.length > 0;
    const isExpanded = state.expandedFolders.has(entry.path);
    
    // Get relative path for git status lookup
    const workDir = state.settings.work_dir || '';
    const relPath = entry.path.replace(workDir + '/', '').replace(workDir, '');
    const gitStatus = getGitStatusForPath(relPath);
    
    const item = document.createElement('div');
    item.className = 'file-tree-item-wrapper';
    
    const row = document.createElement('div');
    row.className = 'file-tree-item' + (gitStatus ? ` git-${gitStatus}` : '');
    row.style.paddingLeft = `${12 + depth * 16}px`;
    
    // Toggle button for folders
    const toggle = document.createElement('button');
    toggle.className = 'file-tree-toggle' + (isExpanded ? ' expanded' : '');
    toggle.innerHTML = `<svg viewBox="0 0 24 24" width="12" height="12"><path d="M9 18l6-6-6-6" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>`;
    toggle.disabled = !hasChildren;
    toggle.onclick = (e) => {
      e.stopPropagation();
      toggleFolder(entry.path);
    };
    row.appendChild(toggle);
    
    // Icon
    const icon = document.createElement('span');
    icon.className = 'file-tree-icon';
    if (entry.is_dir) {
      icon.classList.add(isExpanded ? 'folder-open' : 'folder');
      icon.innerHTML = `<svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor"><path d="M4 7h6l2 2h8v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>`;
    } else {
      icon.innerHTML = getFileIcon(entry.name);
    }
    row.appendChild(icon);
    
    // Name
    const name = document.createElement('span');
    name.className = 'file-tree-name';
    name.textContent = entry.name;
    row.appendChild(name);
    
    // Git status indicator dot
    if (gitStatus && !entry.is_dir) {
      const statusDot = document.createElement('span');
      statusDot.className = `file-tree-status ${gitStatus}`;
      row.appendChild(statusDot);
    }
    
    // Click handler
    row.onclick = () => {
      if (entry.is_dir) {
        toggleFolder(entry.path);
      } else {
        openFile(entry.path, entry.name);
      }
      
      // Update selection
      document.querySelectorAll('.file-tree-item').forEach(el => el.classList.remove('selected'));
      row.classList.add('selected');
    };
    
    item.appendChild(row);
    
    // Children container for folders
    if (entry.is_dir && entry.children) {
      const children = document.createElement('div');
      children.className = 'file-tree-children' + (isExpanded ? ' expanded' : '');
      
      if (isExpanded) {
        const sortedChildren = [...entry.children].sort((a, b) => {
          if (a.is_dir === b.is_dir) {
            return a.name.localeCompare(b.name);
          }
          return a.is_dir ? -1 : 1;
        });
        
        sortedChildren.forEach(child => {
          const childItem = createFileTreeItem(child, depth + 1);
          children.appendChild(childItem);
        });
      }
      
      item.appendChild(children);
    }
    
    return item;
  }
  
  function toggleFolder(path) {
    if (state.expandedFolders.has(path)) {
      state.expandedFolders.delete(path);
    } else {
      state.expandedFolders.add(path);
    }
    renderFileTree();
  }
  
  function getFileIcon(filename) {
    const ext = filename.split('.').pop()?.toLowerCase();
    
    // File icon based on extension
    const iconMap = {
      js: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#f7df1e" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="8" fill="#f7df1e" font-weight="bold">JS</text></svg>',
      ts: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#3178c6" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="8" fill="#3178c6" font-weight="bold">TS</text></svg>',
      jsx: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#61dafb" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="7" fill="#61dafb" font-weight="bold">JSX</text></svg>',
      tsx: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#61dafb" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="7" fill="#61dafb" font-weight="bold">TSX</text></svg>',
      json: '<svg viewBox="0 0 24 24" width="14" height="14"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" fill="none" stroke="#666" stroke-width="1.5"/><path d="M14 2v6h6" fill="none" stroke="#666" stroke-width="1.5"/><text x="12" y="18" text-anchor="middle" font-size="7" fill="#666" font-weight="bold">{}</text></svg>',
      md: '<svg viewBox="0 0 24 24" width="14" height="14"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" fill="none" stroke="#666" stroke-width="1.5"/><path d="M14 2v6h6" fill="none" stroke="#666" stroke-width="1.5"/><text x="12" y="17" text-anchor="middle" font-size="8" fill="#666" font-weight="bold">M</text></svg>',
      css: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#264de4" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="8" fill="#264de4" font-weight="bold">CSS</text></svg>',
      html: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#e34c26" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="7" fill="#e34c26" font-weight="bold">&lt;/&gt;</text></svg>',
      py: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#3776ab" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="8" fill="#3776ab" font-weight="bold">Py</text></svg>',
      rs: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#dea584" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="8" fill="#dea584" font-weight="bold">RS</text></svg>',
      go: '<svg viewBox="0 0 24 24" width="14" height="14"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="#00add8" stroke-width="1.5"/><text x="12" y="16" text-anchor="middle" font-size="8" fill="#00add8" font-weight="bold">Go</text></svg>',
    };
    
    return iconMap[ext] || '<svg viewBox="0 0 24 24" width="14" height="14"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M14 2v6h6" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>';
  }
  
  async function openFile(filePath, fileName) {
    const workDir = state.settings.work_dir;
    if (!workDir) return;
    
    try {
      // Check if file is already open
      const existingTab = state.openTabs.find(tab => tab.path === filePath && tab.type === TAB_TYPES.FILE);
      if (existingTab) {
        activateTab(filePath);
        return;
      }
      
      // Read file content
      const relPath = filePath.replace(workDir + '/', '');
      const content = await invoke('read_file', { workDir, filePath: relPath });
      
      // Add to tabs
      const tab = { path: filePath, name: fileName, content, type: TAB_TYPES.FILE };
      state.openTabs.push(tab);
      state.activeTab = filePath;
      
      renderTabs();
      showFileView(tab);
    } catch (err) {
      showError('Failed to open file: ' + err.message);
    }
  }
  
  function renderTabs() {
    if (state.openTabs.length === 0) {
      elements.tabBar.classList.add('hidden');
      return;
    }
    
    elements.tabBar.classList.remove('hidden');
    elements.tabList.innerHTML = state.openTabs.map(tab => `
      <div class="tab-item ${tab.path === state.activeTab ? 'active' : ''}" data-path="${escapeHtml(tab.path)}">
        <span class="tab-item-name">${escapeHtml(tab.name)}</span>
        <button class="tab-item-close" data-path="${escapeHtml(tab.path)}" title="Close">
          <svg viewBox="0 0 24 24" width="12" height="12">
            <path d="M18 6L6 18M6 6l12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
        </button>
      </div>
    `).join('');
    
    // Add click handlers
    elements.tabList.querySelectorAll('.tab-item').forEach(tabEl => {
      tabEl.addEventListener('click', (e) => {
        if (!e.target.closest('.tab-item-close')) {
          activateTab(tabEl.dataset.path);
        }
      });
    });
    
    elements.tabList.querySelectorAll('.tab-item-close').forEach(btn => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        closeTab(btn.dataset.path);
      });
    });
  }
  
  function activateTab(path) {
    state.activeTab = path;
    renderTabs();
    
    const tab = state.openTabs.find(t => t.path === path);
    if (tab) {
      if (tab.type === TAB_TYPES.FILE) {
        showFileView(tab);
      } else {
        showChatView(tab.session);
      }
    }
  }
  
  function closeTab(path) {
    const index = state.openTabs.findIndex(t => t.path === path);
    if (index === -1) return;
    
    const tab = state.openTabs[index];
    state.openTabs.splice(index, 1);
    
    if (state.activeTab === path) {
      exitEditMode();
      if (state.openTabs.length > 0) {
        const newIndex = Math.min(index, state.openTabs.length - 1);
        state.activeTab = state.openTabs[newIndex].path;
        const nextTab = state.openTabs[newIndex];
        if (nextTab.type === TAB_TYPES.FILE) {
          showFileView(nextTab);
        } else {
          showChatView(nextTab.session);
        }
      } else {
        state.activeTab = null;
        state.currentSession = null;
        elements.fileView.classList.add('hidden');
        elements.chatView.classList.add('hidden');
        elements.emptyState.classList.remove('hidden');
      }
    }
    
    renderTabs();
  }
  
  function showChatView(session) {
    state.currentSession = session;
    elements.emptyState.classList.add('hidden');
    elements.fileView.classList.add('hidden');
    elements.chatView.classList.remove('hidden');
    elements.chatTitle.textContent = session.title;
    
    // Load messages if not already loaded
    if (elements.messages.innerHTML === '' || elements.messages.innerHTML.includes('No messages yet')) {
      loadSessionMessages(session);
    }
  }
  
  function showFileView(tab) {
    elements.emptyState.classList.add('hidden');
    elements.chatView.classList.add('hidden');
    elements.fileView.classList.remove('hidden');
    
    // Detect language for highlighting
    const ext = tab.name.split('.').pop()?.toLowerCase();
    const langMap = {
      js: 'javascript', ts: 'typescript', jsx: 'javascript', tsx: 'typescript',
      py: 'python', rs: 'rust', go: 'go', java: 'java', cpp: 'cpp', c: 'c',
      html: 'xml', xml: 'xml', css: 'css', scss: 'scss', sass: 'sass',
      json: 'json', md: 'markdown', yaml: 'yaml', yml: 'yaml', toml: 'toml',
      sh: 'bash', bash: 'bash', zsh: 'bash'
    };
    const lang = langMap[ext] || '';
    
    // Escape HTML
    const escaped = tab.content
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
    
    elements.fileViewContent.innerHTML = `<pre><code class="hljs ${lang}">${escaped}</code></pre>`;
    
    // Apply syntax highlighting
    if (typeof hljs !== 'undefined' && lang) {
      elements.fileViewContent.querySelectorAll('pre code').forEach(block => {
        hljs.highlightElement(block);
      });
    }
  }
  
  function hideFileView() {
    elements.fileView.classList.add('hidden');
    exitEditMode();
    
    if (state.currentSession) {
      elements.chatView.classList.remove('hidden');
    } else {
      elements.emptyState.classList.remove('hidden');
    }
  }
  
  function enterEditMode() {
    if (!state.activeTab) return;
    
    const tab = state.openTabs.find(t => t.path === state.activeTab);
    if (!tab) return;
    
    // Show editor
    elements.fileViewContent.classList.add('hidden');
    elements.fileViewEditor.classList.remove('hidden');
    elements.fileEditorTextarea.value = tab.content;
    elements.fileEditorTextarea.focus();
    
    // Update buttons
    elements.btnEditFile.classList.add('hidden');
    elements.btnSaveFile.classList.remove('hidden');
    elements.btnCancelEdit.classList.remove('hidden');
    
    // Track original content for comparison
    state.editOriginalContent = tab.content;
  }
  
  function exitEditMode() {
    // Hide editor
    elements.fileViewContent.classList.remove('hidden');
    elements.fileViewEditor.classList.add('hidden');
    
    // Update buttons
    elements.btnEditFile.classList.remove('hidden');
    elements.btnSaveFile.classList.add('hidden');
    elements.btnCancelEdit.classList.add('hidden');
    
    state.editOriginalContent = null;
  }
  
  async function saveFile() {
    if (!state.activeTab) return;
    
    const tab = state.openTabs.find(t => t.path === state.activeTab);
    if (!tab) return;
    
    const newContent = elements.fileEditorTextarea.value;
    const workDir = state.settings.work_dir;
    if (!workDir) return;
    
    try {
      const relPath = tab.path.replace(workDir + '/', '');
      await invoke('write_file', { workDir, filePath: relPath, content: newContent });
      
      // Update tab content
      tab.content = newContent;
      
      // Exit edit mode and refresh view
      exitEditMode();
      showFileView(tab);
      
      showSuccess('File saved');
      
      // Refresh git status
      loadFileTree();
    } catch (err) {
      showError('Failed to save file: ' + err.message);
    }
  }
  
  function cancelEdit() {
    exitEditMode();
  }
  
  function toggleExplorer() {
    state.explorerCollapsed = !state.explorerCollapsed;
    if (elements.appContainer) {
      elements.appContainer.classList.toggle('explorer-collapsed', state.explorerCollapsed);
    }
    // Update toggle button visibility
    if (elements.btnToggleExplorer) {
      elements.btnToggleExplorer.classList.toggle('hidden', !state.explorerCollapsed);
    }
  }

  async function loadConfig() {
    try {
      const path = state.settings.config_file || null;
      state.config = await invoke('config_load', { path });
      elements.configEditor.value = state.config.raw;
    } catch (err) {
      const message = err?.message || err || 'Failed to load config';
      showError(`Failed to load config: ${message}`);
    }
  }

  async function loadMcp() {
    try {
      const path = state.settings.mcp_config_files?.[0] || null;
      state.mcp = await invoke('mcp_load', { path });
      elements.mcpEditor.value = state.mcp.raw;
    } catch (err) {
      const message = err?.message || err || 'Failed to load MCP';
      showError(`Failed to load MCP: ${message}`);
    }
  }

  async function loadSkills() {
    try {
      const payload = await invoke('skills_list', {
        workDir: state.settings.work_dir || null,
        skillsDir: state.settings.skills_dir || null
      });
      state.skills = payload;
      renderSkills();
    } catch (err) {
      const message = err?.message || err || 'Failed to load skills';
      showError(`Failed to load skills: ${message}`);
    }
  }

  async function loadSessions(allSessions = false) {
    try {
      const workDir = state.settings.work_dir || state.paths?.work_dir;
      // If allSessions is true, pass null to get all sessions
      // Otherwise filter by work_dir
      const sessions = await invoke('session_list', { 
        workDir: allSessions ? null : workDir 
      });
      state.sessions = sessions || [];
      const validIds = new Set(state.sessions.map(s => s.id));
      state.settings.pinned_sessions = (state.settings.pinned_sessions || []).filter(id => validIds.has(id));
      renderSessions();
    } catch (err) {
      const message = err?.message || err || 'Failed to load sessions';
      showError(`Failed to load sessions: ${message}`);
      state.sessions = [];
      renderSessions();
    }
  }

  async function loadModels() {
    if (!state.isLoggedIn) {
      state.models = [];
      renderModels();
      return;
    }
    
    try {
      const config = await invoke('auth_get_config');
      const models = await invoke('llm_fetch_models', { authConfig: config });
      state.models = models || [];
      renderModels();
    } catch (err) {
      const message = err?.message || err || 'Failed to load models';
      showError(`Failed to load models: ${message}`);
      state.models = [];
      renderModels();
    }
  }

  async function checkAuthStatus() {
    try {
      const status = await invoke('auth_check_status');
      state.isLoggedIn = status.is_logged_in;
      state.authMode = status.mode; // 'oauth' | 'api_key' | 'none'
      
      // Load auth config for settings display
      state.authConfig = await invoke('auth_get_config');
      
      if (state.isLoggedIn && status.mode === 'oauth') {
        await loadUserProfile();
      } else if (state.isLoggedIn && status.mode === 'api_key') {
        // Set a simple user object for API key mode
        state.user = { 
          mode: 'api_key',
          total_label: 'API Key Mode',
          total_percent: 0,
          total_reset: '',
          limit_label: 'Connected',
          limit_percent: 0,
          limit_reset: ''
        };
      }
      updateUserBar();
    } catch (err) {
      const message = err?.message || err || 'Failed to check auth status';
      showError(`Failed to check auth status: ${message}`);
    }
  }

  function renderModels() {
    const models = state.models;
    
    elements.settingDefaultModel.innerHTML = models.map(m => 
      `<option value="${m.id}">${m.id}</option>`
    ).join('') || '<option value="">No models available</option>';
    
    // Check if current model is still valid, otherwise reset to first available
    const currentModelValid = state.settings.model && models.find(m => m.id === state.settings.model);
    const currentModel = currentModelValid ? state.settings.model : (models[0]?.id);
    
    // Update settings if model was invalid
    if (!currentModelValid && models.length > 0) {
      state.settings.model = currentModel;
    }
    
    if (currentModel && models.find(m => m.id === currentModel)) {
      elements.settingDefaultModel.value = currentModel;
    }
    
    elements.modelLabel.textContent = currentModel || 'Select model';
    
    elements.modelList.innerHTML = models.map(m => {
      const capabilities = [];
      if (m.supports_reasoning) capabilities.push('thinking');
      if (m.supports_image_in) capabilities.push('vision');
      return `
        <div class="list-item">
          <strong>${m.id}</strong>
          <span>${m.context_length.toLocaleString()} tokens · ${capabilities.join(', ') || 'standard'}</span>
        </div>
      `;
    }).join('') || '<div class="list-item">Login to see available models</div>';
    
    elements.modelOptions.innerHTML = models.map(m => `
      <button class="model-option" data-model="${m.id}">
        <svg viewBox="0 0 24 24" width="16" height="16">
          <rect x="3" y="4" width="18" height="12" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/>
        </svg>
        <span>${m.id}</span>
      </button>
    `).join('');
    
    $$('.model-option', elements.modelOptions).forEach(btn => {
      btn.addEventListener('click', () => {
        const model = btn.dataset.model;
        state.settings.model = model;
        elements.modelLabel.textContent = model;
        closeModals();
      });
    });
  }

  function renderSkills() {
    const skills = state.skills?.skills || [];
    elements.skillsList.innerHTML = skills.map(s => `
      <div class="list-item">
        <strong>${s.name}</strong>
        <span>${s.description || s.path}</span>
      </div>
    `).join('') || '<div class="list-item">No skills found</div>';
  }

  function renderSessions() {
    if (state.sessions.length === 0) {
      elements.sessionList.innerHTML = '<div style="padding: 20px; text-align: center; color: var(--text-muted); font-size: 13px;">No sessions yet</div>';
      return;
    }

    const pinnedSet = new Set(state.settings.pinned_sessions || []);
    const ordered = [
      ...state.sessions.filter(s => pinnedSet.has(s.id)),
      ...state.sessions.filter(s => !pinnedSet.has(s.id)),
    ];

    elements.sessionList.innerHTML = ordered.map((s) => {
      const date = new Date(s.updated_at * 1000);
      const timeStr = date.toLocaleDateString() + ' ' + date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
      const isActive = state.currentSession?.id === s.id;
      const isPinned = pinnedSet.has(s.id);
      const actionsVisible = state.isEditingSessions ? 'visible' : '';
      return `
        <div class="session-row">
          <button class="session-item ${isActive ? 'active' : ''}" data-id="${s.id}">
            <div class="session-title">${escapeHtml(s.title)}</div>
            <div class="session-meta">${timeStr}</div>
          </button>
          <div class="session-actions ${actionsVisible}">
            <button class="session-action pin ${isPinned ? 'active' : ''}" data-id="${s.id}" data-action="pin" title="${isPinned ? 'Unpin' : 'Pin'}">
              <svg viewBox="0 0 24 24" width="14" height="14">
                <path d="M12 17v5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                <path d="M9 3h6l1 7-4 4-4-4 1-7Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>
                <path d="M8 10h8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              </svg>
            </button>
            <button class="session-action delete" data-id="${s.id}" data-action="delete" title="Delete">
              <svg viewBox="0 0 24 24" width="14" height="14">
                <path d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              </svg>
            </button>
          </div>
        </div>
      `;
    }).join('');
    
    $$('.session-item', elements.sessionList).forEach(item => {
      item.addEventListener('click', () => {
        const sessionId = item.dataset.id;
        openSession(sessionId);
      });
    });

    if (state.isEditingSessions) {
      $$('.session-action', elements.sessionList).forEach(btn => {
        btn.addEventListener('click', async (e) => {
          e.stopPropagation();
          const sessionId = btn.dataset.id;
          const action = btn.dataset.action;
          if (action === 'pin') {
            togglePinnedSession(sessionId);
            renderSessions();
            await persistSettings();
          }
          if (action === 'delete') {
            await deleteSession(sessionId);
          }
        });
      });
    }
  }

  function togglePinnedSession(sessionId) {
    const list = state.settings.pinned_sessions || [];
    const idx = list.indexOf(sessionId);
    if (idx >= 0) {
      list.splice(idx, 1);
    } else {
      list.unshift(sessionId);
    }
    state.settings.pinned_sessions = list;
  }

  async function persistSettings() {
    await invoke('gui_settings_save', {
      path: null,
      settings: state.settings,
    });
  }

  async function deleteSession(sessionId) {
    const session = state.sessions.find(s => s.id === sessionId);
    if (!session) return;
    try {
      await invoke('session_delete', {
        workDir: session.work_dir,
        sessionId: session.id
      });
      state.sessions = state.sessions.filter(s => s.id !== sessionId);
      state.settings.pinned_sessions = (state.settings.pinned_sessions || []).filter(id => id !== sessionId);
      if (state.currentSession?.id === sessionId) {
        closeChat();
      } else {
        renderSessions();
      }
      await persistSettings();
    } catch (err) {
      const message = err?.message || err || 'Failed to delete session';
      showError(message);
    }
  }

  function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  async function openSession(sessionId) {
    const session = state.sessions.find(s => s.id === sessionId);
    if (!session) return;
    
    // Check if session is already open
    const existingTab = state.openTabs.find(tab => tab.path === sessionId && tab.type === TAB_TYPES.SESSION);
    if (existingTab) {
      activateTab(sessionId);
      return;
    }
    
    state.currentSession = session;
    if (session.work_dir) {
      state.settings.work_dir = session.work_dir;
      updateUI();
    }
    state.messages = [];
    
    // Add to tabs
    const tab = { path: sessionId, name: session.title, type: TAB_TYPES.SESSION, session: session };
    state.openTabs.push(tab);
    state.activeTab = sessionId;
    
    renderTabs();
    showChatView(session);
    
    await loadSessionMessages(session);
    renderSessions();
  }

  async function loadSessionMessages(session) {
    elements.messages.innerHTML = '';
    currentMessageEl = null;
    currentTextBuffer = '';
    toolMessages.clear();
    try {
      const messages = await invoke('session_messages', {
        workDir: session.work_dir,
        sessionId: session.id
      });
      
      if (!messages || messages.length === 0) {
        elements.messages.innerHTML = '<div style="text-align: center; color: var(--text-muted); padding: 40px;">No messages yet</div>';
        return;
      }
      
      messages.forEach(msg => {
        const msgEl = createMessageElement(msg.role, msg.content);
        elements.messages.appendChild(msgEl);
      });
      
      scrollToBottom();
    } catch (err) {
      const errorMsg = err?.message || err || 'Unknown error';
      elements.messages.innerHTML = `<div style="text-align: center; color: var(--error); padding: 40px;">Failed to load messages: ${errorMsg}</div>`;
    }
  }

  function closeChat() {
    // Close the current session tab
    if (state.currentSession) {
      closeTab(state.currentSession.id);
    }
    
    state.currentSession = null;
    state.messages = [];
    currentMessageEl = null;
    currentTextBuffer = '';
    toolMessages.clear();
    renderSessions();
  }
  
  function createNewSessionTab() {
    const sessionId = generateId();
    const title = 'New Session';
    
    // Add to tabs
    const tab = { path: sessionId, name: title, type: TAB_TYPES.SESSION, session: null, isNew: true };
    state.openTabs.push(tab);
    state.activeTab = sessionId;
    state.currentSession = null;
    
    renderTabs();
    showNewSessionView();
  }
  
  function showNewSessionView() {
    elements.chatView.classList.add('hidden');
    elements.fileView.classList.add('hidden');
    elements.emptyState.classList.remove('hidden');
    elements.promptInput.focus();
  }

  async function sendMessage(text, fromChat = false) {
    hideAutocomplete();
    
    if (!text.trim() || state.isStreaming) return;
    
    if (!state.isLoggedIn) {
      showError('Please login first');
      openLoginModal();
      return;
    }
    
    const inputEl = fromChat ? elements.chatInput : elements.promptInput;
    inputEl.value = '';
    
    // Check if current tab is a "New Session" tab (empty session)
    const currentTab = state.openTabs.find(t => t.path === state.activeTab);
    if (currentTab && currentTab.isNew) {
      // Convert the "New Session" tab to a real session
      await startNewSessionInTab(text, currentTab);
      return;
    }
    
    if (!state.currentSession) {
      await startNewSession(text);
      return;
    }
    
    // Add user message
    const userMsg = createMessageElement('user', text);
    elements.messages.appendChild(userMsg);
    scrollToBottom();
    
    // Reset streaming state
    currentMessageEl = null;
    currentTextBuffer = '';
    state.isStreaming = true;
    
    enableInputs(false);
    showLoading('Kimi is thinking...');
    
    try {
      const sessionWorkDir = state.currentSession?.work_dir || state.settings.work_dir || state.paths?.work_dir || null;
      await invoke('chat_stream', {
        sessionId: state.currentSession.id,
        message: text,
        settings: {
          ...state.settings,
          work_dir: sessionWorkDir,
        },
      });
    } catch (err) {
      const errorMsg = err?.message || err || 'Failed to send message';
      showError(errorMsg);
      finishStreaming();
    }
  }

  async function startNewSession(prompt) {
    if (!state.isLoggedIn) {
      showError('Please login first');
      openLoginModal();
      return;
    }
    
    const sessionId = generateId();
    const title = prompt.length > 50 ? prompt.slice(0, 47) + '...' : prompt;
    
    const workDir = state.settings.work_dir || state.paths?.work_dir || null;
    state.settings.work_dir = workDir;
    state.currentSession = {
      id: sessionId,
      title: title,
      work_dir: workDir,
      updated_at: Date.now() / 1000,
    };
    
    // Add to tabs
    const tab = { path: sessionId, name: title, type: TAB_TYPES.SESSION, session: state.currentSession };
    state.openTabs.push(tab);
    state.activeTab = sessionId;
    renderTabs();
    
    elements.emptyState.classList.add('hidden');
    elements.fileView.classList.add('hidden');
    elements.chatView.classList.remove('hidden');
    elements.messages.innerHTML = '';
    elements.chatTitle.textContent = title;
    toolMessages.clear();
    
    // Add user message
    const userMsg = createMessageElement('user', prompt);
    elements.messages.appendChild(userMsg);
    
    currentMessageEl = null;
    currentTextBuffer = '';
    state.isStreaming = true;
    
    enableInputs(false);
    showLoading('Kimi is thinking...');
    
    try {
      const sessionWorkDir = state.currentSession?.work_dir || state.settings.work_dir || state.paths?.work_dir || null;
      await invoke('chat_stream', {
        sessionId: sessionId,
        message: prompt,
        settings: {
          ...state.settings,
          work_dir: sessionWorkDir,
        },
      });
      
      // Add to sessions list
      state.sessions.unshift({
        ...state.currentSession,
        updated_at: Date.now() / 1000,
      });
      renderSessions();
      
    } catch (err) {
      const errorMsg = err?.message || err || 'Failed to start session';
      showError(errorMsg);
      finishStreaming();
    }
  }
  
  async function startNewSessionInTab(prompt, existingTab) {
    const sessionId = existingTab.path;
    const title = prompt.length > 50 ? prompt.slice(0, 47) + '...' : prompt;
    
    const workDir = state.settings.work_dir || state.paths?.work_dir || null;
    state.settings.work_dir = workDir;
    state.currentSession = {
      id: sessionId,
      title: title,
      work_dir: workDir,
      updated_at: Date.now() / 1000,
    };
    
    // Update the existing tab
    existingTab.name = title;
    existingTab.session = state.currentSession;
    existingTab.isNew = false;
    state.activeTab = sessionId;
    renderTabs();
    
    elements.emptyState.classList.add('hidden');
    elements.fileView.classList.add('hidden');
    elements.chatView.classList.remove('hidden');
    elements.messages.innerHTML = '';
    elements.chatTitle.textContent = title;
    toolMessages.clear();
    
    // Add user message
    const userMsg = createMessageElement('user', prompt);
    elements.messages.appendChild(userMsg);
    
    currentMessageEl = null;
    currentTextBuffer = '';
    state.isStreaming = true;
    
    enableInputs(false);
    showLoading('Kimi is thinking...');
    
    try {
      const sessionWorkDir = state.currentSession?.work_dir || state.settings.work_dir || state.paths?.work_dir || null;
      await invoke('chat_stream', {
        sessionId: sessionId,
        message: prompt,
        settings: {
          ...state.settings,
          work_dir: sessionWorkDir,
        },
      });
      
      // Add to sessions list
      state.sessions.unshift({
        ...state.currentSession,
        updated_at: Date.now() / 1000,
      });
      renderSessions();
      
    } catch (err) {
      const errorMsg = err?.message || err || 'Failed to start session';
      showError(errorMsg);
      finishStreaming();
    }
  }

  function generateId() {
    return Date.now().toString(36) + Math.random().toString(36).substr(2);
  }

  function updateUserBar() {
    if (state.isLoggedIn && state.user) {
      elements.userStatus.innerHTML = '';
      
      if (state.authMode === 'api_key') {
        // API Key mode - show simple connected status
        const apiKeyRow = document.createElement('div');
        apiKeyRow.className = 'quota-row';
        apiKeyRow.innerHTML = `
          <div class="quota-header">
            <span class="quota-label">API Key Mode</span>
            <span class="quota-reset">Connected</span>
          </div>
        `;
        elements.userStatus.appendChild(apiKeyRow);
      } else {
        // OAuth mode - show usage quotas
        // Weekly usage row
        const totalRow = document.createElement('div');
        totalRow.className = 'quota-row';
        totalRow.innerHTML = `
          <div class="quota-header">
            <span class="quota-label">${state.user.total_label || 'Weekly usage'}</span>
            <span class="quota-reset">${state.user.total_reset || ''}</span>
          </div>
          <div class="quota-main">
            <span class="quota-percent">${Math.round(state.user.total_percent || 0)}%</span>
            <div class="quota-bar"><div class="quota-fill" style="width: ${state.user.total_percent || 0}%"></div></div>
          </div>
        `;
        elements.userStatus.appendChild(totalRow);
        
        // Rate limit row
        const limitRow = document.createElement('div');
        limitRow.className = 'quota-row';
        limitRow.innerHTML = `
          <div class="quota-header">
            <span class="quota-label">${state.user.limit_label || 'Rate limit'}</span>
            <span class="quota-reset">${state.user.limit_reset || ''}</span>
          </div>
          <div class="quota-main">
            <span class="quota-percent">${Math.round(state.user.limit_percent || 0)}%</span>
            <div class="quota-bar"><div class="quota-fill" style="width: ${state.user.limit_percent || 0}%"></div></div>
          </div>
        `;
        elements.userStatus.appendChild(limitRow);
      }
      
      const hint = document.createElement('div');
      hint.className = 'user-status-hint';
      hint.textContent = 'Click to logout';
      elements.userStatus.appendChild(hint);
      elements.userBar.classList.add('logged-in');
      elements.userBar.onclick = handleLogout;
      
      // Show file explorer for logged in users
      document.querySelector('.app').classList.add('logged-in');
      
      // Load file tree if work_dir is set
      if (state.settings.work_dir) {
        loadFileTree();
      }
    } else {
      elements.userStatus.textContent = 'Click to login';
      elements.userBar.classList.remove('logged-in');
      elements.userBar.onclick = openLoginModal;
      
      // Hide file explorer for logged out users
      document.querySelector('.app').classList.remove('logged-in');
    }
  }
  
  async function loadUserProfile() {
    if (!state.isLoggedIn) return;
    
    try {
      const user = await invoke('oauth_get_user');
      state.user = user;
      updateUserBar();
    } catch (err) {
      const message = err?.message || err || 'Failed to load user profile';
      state.user = null;
      updateUserBar();
      showError(`Failed to load user profile: ${message}`);
    }
  }

  function syncYoloSwitches() {
    const value = !!state.settings.yolo;
    if (elements.yoloSwitchMain) elements.yoloSwitchMain.checked = value;
    if (elements.yoloSwitchChat) elements.yoloSwitchChat.checked = value;
  }

  function showLoginPrompt() {
    // Replace empty-state content with login prompt
    elements.emptyState.innerHTML = `
      <div class="logo">
        <div class="logo-pill">
          <svg class="logo-kimi" viewBox="0 0 55 24" xmlns="http://www.w3.org/2000/svg">
            <title>Kimi</title>
            <path fill="currentColor" fill-rule="evenodd" clip-rule="evenodd" d="M13.998 2h4.277L15.76 7.645a3.9 3.9 0 01-2.297 2.104h2.1v.01a3.834 3.834 0 013.548 3.83V22h-3.825V11.852a2.99 2.99 0 01-2.713 1.736H5.825V22H2V2.035h3.825v7.714h4.787L13.998 2zM25.93 2h-3.815v20h3.815V2zm23.468 0h3.815v20h-3.815V2zM28.936 22V2h3.855l4.888 7.828L42.557 2h3.836v20h-3.815V9.183l-4.896 7.855-4.93-7.898V22h-3.816z"></path>
          </svg>
        </div>
        <span class="logo-code">Code</span>
      </div>
      <p style="text-align: center; color: var(--text-secondary); margin: 24px 0; font-size: 15px; max-width: 320px;">
        Connect to Kimi to start coding with AI
      </p>
      <button class="btn-primary btn-large" id="btn-welcome-login" style="min-width: 200px;">
        Get Started
      </button>
    `;
    
    // Add click handler for the login button
    const loginBtn = document.getElementById('btn-welcome-login');
    if (loginBtn) {
      loginBtn.addEventListener('click', openLoginModal);
    }
    
    // Hide the preview badge and controls row if they exist
    const previewBadge = elements.emptyState.querySelector('.preview-badge');
    if (previewBadge) previewBadge.style.display = 'none';
  }

  function updateUI() {
    elements.settingWorkdir.value = state.settings.work_dir || state.paths?.work_dir || '';
    elements.settingConfig.value = state.settings.config_file || state.paths?.config || '';
    elements.settingMcp.value = (state.settings.mcp_config_files || []).join(', ');
    elements.settingSkills.value = state.settings.skills_dir || '';
    elements.settingThinking.checked = state.settings.thinking || false;
    elements.settingYolo.checked = state.settings.yolo || false;
    
    // Load auth config into settings
    if (state.authConfig) {
      elements.settingApiKey.value = state.authConfig.api_key || '';
      elements.settingApiBase.value = state.authConfig.api_base || '';
    }
    
    const workDir = state.settings.work_dir || state.paths?.work_dir;
    if (workDir) {
      const parts = workDir.split('/');
      elements.folderLabel.textContent = parts[parts.length - 1] || workDir;
    } else {
      elements.folderLabel.textContent = 'Select folder';
    }
    
    syncYoloSwitches();
  }

  // Login/Logout
  function openLoginModal() {
    if (state.isLoggedIn) return;
    elements.loginModal.classList.add('open');
    resetLoginModal();
  }

  function closeLoginModal() {
    elements.loginModal.classList.remove('open');
  }

  function resetLoginModal() {
    // Show method selection by default
    if (elements.loginMethods) elements.loginMethods.classList.remove('hidden');
    if (elements.loginOauthFlow) elements.loginOauthFlow.classList.add('hidden');
    if (elements.loginApikeyForm) elements.loginApikeyForm.classList.add('hidden');
    
    // Reset OAuth flow
    if (elements.oauthStart) elements.oauthStart.classList.remove('hidden');
    if (elements.oauthProgress) elements.oauthProgress.classList.add('hidden');
    if (elements.loginUserCode) elements.loginUserCode.textContent = '';
    if (elements.loginStatus) elements.loginStatus.textContent = 'Waiting for authorization...';
    
    // Reset API key form
    if (elements.apiKeyInput) elements.apiKeyInput.value = '';
    if (elements.apiBaseInput) elements.apiBaseInput.value = '';
  }
  
  async function showOauthLogin() {
    if (elements.loginMethods) elements.loginMethods.classList.add('hidden');
    if (elements.loginOauthFlow) elements.loginOauthFlow.classList.remove('hidden');
    if (elements.loginApikeyForm) elements.loginApikeyForm.classList.add('hidden');
    
    // Auto-start the OAuth login process
    await startLogin();
  }
  
  function showApikeyLogin() {
    if (elements.loginMethods) elements.loginMethods.classList.add('hidden');
    if (elements.loginOauthFlow) elements.loginOauthFlow.classList.add('hidden');
    if (elements.loginApikeyForm) elements.loginApikeyForm.classList.remove('hidden');
  }
  
  function showLoginMethods() {
    resetLoginModal();
  }

  async function startLogin() {
    if (elements.oauthStart) elements.oauthStart.classList.add('hidden');
    if (elements.oauthProgress) elements.oauthProgress.classList.remove('hidden');
    
    try {
      const result = await invoke('oauth_start_login');
      
      if (elements.loginUserCode) {
        elements.loginUserCode.textContent = result.user_code;
      }
      
      // Store verification URL for opening browser
      state.verificationUrl = result.verification_uri_complete;
      
    } catch (err) {
      showError('Login failed: ' + err.message);
      resetLoginModal();
    }
  }
  
  async function saveApiKey() {
    const apiKey = elements.apiKeyInput?.value?.trim();
    const apiBase = elements.apiBaseInput?.value?.trim();
    
    if (!apiKey) {
      showError('Please enter an API key');
      return;
    }
    
    try {
      await invoke('auth_set_api_key', { 
        apiKey: apiKey,
        apiBase: apiBase || null
      });
      
      state.isLoggedIn = true;
      state.authMode = 'api_key';
      closeLoginModal();
      showSuccess('Connected with API key');
      // Reload to properly initialize the app with API key auth
      location.reload();
    } catch (err) {
      showError('Failed to save API key: ' + err.message);
    }
  }

  async function openBrowserForLogin() {
    if (state.verificationUrl) {
      try {
        await invoke('oauth_open_browser', { url: state.verificationUrl });
      } catch (err) {
        // Fallback: copy to clipboard or show manual URL
        showError('Could not open browser. Please visit: ' + state.verificationUrl);
      }
    }
  }

  async function handleLogout() {
    if (!state.isLoggedIn) return;
    if (!confirm('Logout from Kimi?')) return;
    
    try {
      // Clear auth regardless of mode
      await invoke('auth_clear');
      state.isLoggedIn = false;
      state.authMode = null;
      state.models = [];
      updateUserBar();
      renderModels();
      showSuccess('Logged out successfully');
      // Reload to show initial Get Started page
      setTimeout(() => window.location.reload(), 500);
    } catch (err) {
      showError('Logout failed: ' + err.message);
    }
  }

  function closeModals() {
    elements.folderModal.classList.remove('open');
    elements.modelModal.classList.remove('open');
  }
  
  // ================================
  // Autocomplete Functions
  // ================================
  
  function createAutocompleteDropdown() {
    let dropdown = document.getElementById('autocomplete-dropdown');
    if (!dropdown) {
      dropdown = document.createElement('div');
      dropdown.id = 'autocomplete-dropdown';
      dropdown.className = 'autocomplete-dropdown';
      dropdown.style.cssText = `
        position: absolute;
        background: white;
        border: 1px solid var(--border);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-lg);
        max-height: 200px;
        overflow-y: auto;
        z-index: 1000;
        display: none;
        min-width: 200px;
      `;
      document.body.appendChild(dropdown);
    }
    return dropdown;
  }
  
  function showAutocomplete(input, type, query, triggerStart) {
    autocomplete.active = true;
    autocomplete.type = type;
    autocomplete.query = query;
    autocomplete.targetInput = input;
    autocomplete.triggerStart = triggerStart;
    autocomplete.selectedIndex = 0;
    
    const dropdown = createAutocompleteDropdown();
    
    // Calculate position
    const rect = input.getBoundingClientRect();
    const dropdownHeight = 250; // max-height
    const spaceBelow = window.innerHeight - rect.bottom;
    const spaceAbove = rect.top;
    
    // Show above if not enough space below
    if (spaceBelow < dropdownHeight && spaceAbove > dropdownHeight) {
      dropdown.style.top = (rect.top + window.scrollY - dropdownHeight) + 'px';
      dropdown.style.maxHeight = Math.min(dropdownHeight, spaceAbove - 10) + 'px';
    } else {
      dropdown.style.top = (rect.bottom + window.scrollY) + 'px';
      dropdown.style.maxHeight = Math.min(dropdownHeight, spaceBelow - 10) + 'px';
    }
    
    dropdown.style.left = rect.left + 'px';
    dropdown.style.width = rect.width + 'px';
    
    fetchAndRenderSuggestions();
    dropdown.style.display = 'block';
  }
  
  function hideAutocomplete() {
    autocomplete.active = false;
    const dropdown = document.getElementById('autocomplete-dropdown');
    if (dropdown) {
      dropdown.style.display = 'none';
    }
  }
  
  async function fetchAndRenderSuggestions() {
    const dropdown = document.getElementById('autocomplete-dropdown');
    if (!dropdown) return;
    
    let suggestions = [];
    const query = autocomplete.query.toLowerCase();
    
    switch (autocomplete.type) {
      case 'slash':
        suggestions = slashCommands
          .filter(cmd => cmd.name.includes(query) || cmd.aliases.some(a => a.includes(query)))
          .map(cmd => ({
            value: '/' + cmd.name,
            display: '/' + cmd.name,
            description: cmd.description,
            icon: '⌘'
          }));
        break;
        
      case 'skill':
        suggestions = (state.skills?.skills || [])
          .filter(skill => skill.name.toLowerCase().includes(query))
          .map(skill => ({
            value: '$' + skill.name,
            display: '$' + skill.name,
            description: skill.description || skill.path,
            icon: '📋'
          }));
        break;
        
      case 'file':
        if (state.settings.work_dir || state.paths?.work_dir) {
          try {
            const workDir = state.settings.work_dir || state.paths.work_dir;
            const files = await invoke('list_files', { 
              work_dir: workDir, 
              query: query.length > 0 ? query : null 
            });
            suggestions = files.slice(0, 10).map(f => ({
              value: '@' + f,
              display: '@' + f,
              description: 'File',
              icon: '📄'
            }));
          } catch (e) {
            suggestions = [];
          }
        }
        break;
    }
    
    autocomplete.suggestions = suggestions;
    renderAutocompleteDropdown();
  }
  
  function renderAutocompleteDropdown() {
    const dropdown = document.getElementById('autocomplete-dropdown');
    if (!dropdown) return;
    
    if (autocomplete.suggestions.length === 0) {
      dropdown.innerHTML = '<div class="autocomplete-item" style="padding: 8px 12px; color: #999;">No matches</div>';
      return;
    }
    
    dropdown.innerHTML = autocomplete.suggestions.map((s, i) => `
      <div class="autocomplete-item ${i === autocomplete.selectedIndex ? 'selected' : ''}" 
           data-index="${i}"
           style="padding: 8px 12px; cursor: pointer; display: flex; align-items: center; gap: 8px;
                  ${i === autocomplete.selectedIndex ? 'background: var(--accent-light);' : ''}
                  ${i !== autocomplete.suggestions.length - 1 ? 'border-bottom: 1px solid var(--border);' : ''}">
        <span>${s.icon}</span>
        <div style="flex: 1; min-width: 0;">
          <div style="font-weight: 500;">${escapeHtml(s.display)}</div>
          <div style="font-size: 12px; color: #666; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">${escapeHtml(s.description)}</div>
        </div>
      </div>
    `).join('');
    
    // Add click handlers
    dropdown.querySelectorAll('.autocomplete-item').forEach(item => {
      item.addEventListener('click', () => {
        const index = parseInt(item.dataset.index);
        selectAutocompleteSuggestion(index);
      });
    });
  }
  
  function selectAutocompleteSuggestion(index) {
    if (index < 0 || index >= autocomplete.suggestions.length) return;
    
    const suggestion = autocomplete.suggestions[index];
    const input = autocomplete.targetInput;
    const cursorPos = input.selectionStart;
    const textBefore = input.value.substring(0, autocomplete.triggerStart);
    const textAfter = input.value.substring(cursorPos);
    
    input.value = textBefore + suggestion.value + ' ' + textAfter;
    input.focus();
    input.setSelectionRange(
      textBefore.length + suggestion.value.length + 1,
      textBefore.length + suggestion.value.length + 1
    );
    
    hideAutocomplete();
  }
  
  function moveAutocompleteSelection(delta) {
    if (!autocomplete.active) return;
    
    autocomplete.selectedIndex += delta;
    if (autocomplete.selectedIndex < 0) {
      autocomplete.selectedIndex = autocomplete.suggestions.length - 1;
    } else if (autocomplete.selectedIndex >= autocomplete.suggestions.length) {
      autocomplete.selectedIndex = 0;
    }
    
    renderAutocompleteDropdown();
  }
  
  function handleInputKeydown(e) {
    if (!autocomplete.active) {
      // Check for trigger characters
      if (e.key === '/' || e.key === '$' || e.key === '@') {
        const input = e.target;
        const cursorPos = input.selectionStart;
        const textBefore = input.value.substring(0, cursorPos);
        
        // Only trigger at start of input or after whitespace
        if (cursorPos === 0 || textBefore.match(/\s$/)) {
          const type = e.key === '/' ? 'slash' : e.key === '$' ? 'skill' : 'file';
          setTimeout(() => {
            showAutocomplete(input, type, '', cursorPos);
          }, 0);
        }
      }
      return;
    }
    
    // Handle autocomplete navigation
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        moveAutocompleteSelection(1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        moveAutocompleteSelection(-1);
        break;
      case 'Enter':
        e.preventDefault();
        selectAutocompleteSuggestion(autocomplete.selectedIndex);
        break;
      case 'Escape':
        e.preventDefault();
        hideAutocomplete();
        break;
    }
  }
  
  function handleInput(e) {
    if (!autocomplete.active) return;
    
    const input = e.target;
    const cursorPos = input.selectionStart;
    
    // Check if cursor is still after the trigger
    if (cursorPos < autocomplete.triggerStart) {
      hideAutocomplete();
      return;
    }
    
    // Update query
    const newQuery = input.value.substring(autocomplete.triggerStart + 1, cursorPos);
    autocomplete.query = newQuery;
    
    // Hide if space typed
    if (newQuery.includes(' ')) {
      hideAutocomplete();
      return;
    }
    
    fetchAndRenderSuggestions();
  }

  function initEvents() {
    elements.btnNewSession.addEventListener('click', () => {
      createNewSessionTab();
    });
    
    elements.btnEditSessions.addEventListener('click', () => {
      state.isEditingSessions = !state.isEditingSessions;
      elements.btnEditSessions.classList.toggle('active', state.isEditingSessions);
      renderSessions();
    });
    elements.btnCloseChat.addEventListener('click', closeChat);
    
    elements.btnSend.addEventListener('click', () => sendMessage(elements.promptInput.value));
    elements.btnChatSend.addEventListener('click', () => sendMessage(elements.chatInput.value, true));
    
    elements.promptInput.addEventListener('keydown', e => {
      handleInputKeydown(e);
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        e.preventDefault();
        hideAutocomplete();
        sendMessage(elements.promptInput.value);
      }
    });
    elements.promptInput.addEventListener('input', handleInput);
    
    elements.chatInput.addEventListener('keydown', e => {
      handleInputKeydown(e);
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        e.preventDefault();
        hideAutocomplete();
        sendMessage(elements.chatInput.value, true);
      }
    });
    elements.chatInput.addEventListener('input', handleInput);
    
    $$('.action-card').forEach(card => {
      card.addEventListener('click', () => {
        elements.promptInput.value = card.dataset.prompt;
        elements.promptInput.focus();
      });
    });
    
    const bindYoloSwitch = (input) => {
      if (!input) return;
      input.addEventListener('change', () => {
        state.settings.yolo = input.checked;
        elements.settingYolo.checked = state.settings.yolo;
        syncYoloSwitches();
      });
    };
    bindYoloSwitch(elements.yoloSwitchMain);
    bindYoloSwitch(elements.yoloSwitchChat);
    
    elements.btnConfig.addEventListener('click', () => {
      elements.drawerBackdrop.classList.add('open');
    });
    if (elements.btnConfigChat) {
      elements.btnConfigChat.addEventListener('click', () => {
        elements.drawerBackdrop.classList.add('open');
      });
    }
    
    elements.btnFolder.addEventListener('click', () => {
      elements.folderModal.classList.add('open');
      
      const folders = [
        state.paths?.work_dir,
        state.settings.work_dir,
        state.paths?.share_dir
      ].filter(Boolean);
      
      const home = state.paths?.share_dir?.replace('/.kimi', '');
      if (home) {
        folders.push(home + '/Projects');
        folders.push(home + '/Code');
        folders.push(home);
      }
      
      const uniqueFolders = [...new Set(folders)];
      elements.folderList.innerHTML = uniqueFolders.map(f => `
        <button class="folder-item" data-folder="${f}">
          <svg viewBox="0 0 24 24" width="16" height="16">
            <path d="M4 7h6l2 2h8v8a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
          <span>${f}</span>
        </button>
      `).join('');
      
      $$('.folder-item', elements.folderList).forEach(item => {
        item.addEventListener('click', () => {
          state.settings.work_dir = item.dataset.folder;
          const parts = item.dataset.folder.split('/');
          elements.folderLabel.textContent = parts[parts.length - 1];
          updateUI();
          closeModals();
          loadSkills();
          loadSessions();
          loadFileTree();
        });
      });
      
      // Add custom folder handler
      const customInput = $('custom-folder-input');
      const addBtn = $('btn-add-custom-folder');
      
      if (customInput && addBtn) {
        addBtn.addEventListener('click', () => {
          const path = customInput.value.trim();
          if (path) {
            state.settings.work_dir = path;
            const parts = path.split('/');
            elements.folderLabel.textContent = parts[parts.length - 1] || path;
            updateUI();
            closeModals();
            loadSkills();
            loadSessions();
            loadFileTree();
          }
        });
        
        customInput.addEventListener('keydown', (e) => {
          if (e.key === 'Enter') {
            addBtn.click();
          }
        });
      }
      
      // Browse button handler
      const browseBtn = $('btn-browse-folder');
      if (browseBtn) {
        browseBtn.addEventListener('click', async () => {
          try {
            const path = await invoke('pick_folder');
          if (path) {
            state.settings.work_dir = path;
            const parts = path.split('/');
            elements.folderLabel.textContent = parts[parts.length - 1] || path;
            updateUI();
            closeModals();
            loadSkills();
            loadSessions();
            loadFileTree();
          }
        } catch (err) {
            const message = err?.message || err || 'Failed to open folder picker';
            showError(message);
          }
        });
      }
    });
    
    elements.btnModel.addEventListener('click', () => {
      elements.modelModal.classList.add('open');
    });
    
    elements.btnCloseFolder.addEventListener('click', closeModals);
    elements.btnCloseModel.addEventListener('click', closeModals);
    elements.btnCloseLogin.addEventListener('click', closeLoginModal);
    elements.btnCancelLogin.addEventListener('click', closeLoginModal);
    
    elements.folderModal.addEventListener('click', (e) => {
      if (e.target === elements.folderModal) closeModals();
    });
    elements.modelModal.addEventListener('click', (e) => {
      if (e.target === elements.modelModal) closeModals();
    });
    elements.loginModal.addEventListener('click', (e) => {
      if (e.target === elements.loginModal) closeLoginModal();
    });
    
    // Login events
    elements.btnLoginStart.addEventListener('click', startLogin);
    if (elements.btnOpenBrowser) {
      elements.btnOpenBrowser.addEventListener('click', openBrowserForLogin);
    }
    
    // New login method selection events
    if (elements.loginOptionOauth) {
      elements.loginOptionOauth.addEventListener('click', showOauthLogin);
    }
    if (elements.loginOptionApikey) {
      elements.loginOptionApikey.addEventListener('click', showApikeyLogin);
    }
    if (elements.btnBackToMethods) {
      elements.btnBackToMethods.addEventListener('click', showLoginMethods);
    }
    if (elements.btnBackFromApikey) {
      elements.btnBackFromApikey.addEventListener('click', showLoginMethods);
    }
    if (elements.btnSaveApikey) {
      elements.btnSaveApikey.addEventListener('click', saveApiKey);
    }
    if (elements.apiKeyInput) {
      elements.apiKeyInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') saveApiKey();
      });
    }

    // Tool approval events
    if (elements.btnToolApprove) {
      elements.btnToolApprove.addEventListener('click', () => respondToolApproval(true));
    }
    if (elements.btnToolReject) {
      elements.btnToolReject.addEventListener('click', () => respondToolApproval(false));
    }
    if (elements.btnCloseToolApproval) {
      elements.btnCloseToolApproval.addEventListener('click', () => respondToolApproval(false));
    }
    if (elements.toolApprovalModal) {
      elements.toolApprovalModal.addEventListener('click', (e) => {
        if (e.target === elements.toolApprovalModal) {
          respondToolApproval(false);
        }
      });
    }
    
    elements.btnCloseSettings.addEventListener('click', () => elements.drawerBackdrop.classList.remove('open'));
    elements.drawerBackdrop.addEventListener('click', (e) => {
      if (e.target === elements.drawerBackdrop) elements.drawerBackdrop.classList.remove('open');
    });
    
    elements.drawerTabs.forEach(tab => {
      tab.addEventListener('click', () => {
        elements.drawerTabs.forEach(t => t.classList.remove('active'));
        tab.classList.add('active');
        const target = tab.dataset.tab;
        $$('.tab-content').forEach(c => {
          c.classList.toggle('active', c.dataset.tab === target);
        });
      });
    });
    
    elements.btnSaveSettings.addEventListener('click', async () => {
      state.settings.work_dir = elements.settingWorkdir.value || null;
      state.settings.config_file = elements.settingConfig.value || null;
      state.settings.mcp_config_files = elements.settingMcp.value
        .split(',').map(s => s.trim()).filter(Boolean);
      state.settings.skills_dir = elements.settingSkills.value || null;
      state.settings.model = elements.settingDefaultModel.value || null;
      state.settings.yolo = elements.settingYolo.checked;
      
      // Save auth config if provided
      const apiKey = elements.settingApiKey.value?.trim();
      const apiBase = elements.settingApiBase.value?.trim();
      
      if (apiKey) {
        try {
          await invoke('auth_set_api_key', { 
            apiKey: apiKey,
            apiBase: apiBase || null
          });
          // Update local state
          state.authConfig = { mode: 'api_key', api_key: apiKey, api_base: apiBase };
          state.isLoggedIn = true;
          state.authMode = 'api_key';
          state.user = { 
            mode: 'api_key',
            total_label: 'API Key Mode',
            total_percent: 0,
            total_reset: '',
            limit_label: 'Connected',
            limit_percent: 0,
            limit_reset: ''
          };
          updateUserBar();
          loadModels();
        } catch (err) {
          showError('Failed to save API key: ' + err.message);
          return;
        }
      } else if (state.authConfig?.mode === 'api_key' && !apiKey) {
        // API Key was cleared - clear auth
        try {
          await invoke('auth_clear');
          state.isLoggedIn = false;
          state.authMode = null;
          state.user = null;
          state.authConfig = { mode: 'oauth', api_key: null, api_base: null };
          updateUserBar();
          showSuccess('API Key cleared. Please login again.');
          elements.drawerBackdrop.classList.remove('open');
          // Show login modal
          setTimeout(() => openLoginModal(), 500);
          return;
        } catch (err) {
          showError('Failed to clear auth: ' + err.message);
        }
      }
      
      await invoke('gui_settings_save', { 
        path: null, 
        settings: state.settings 
      });
      
      await loadConfig();
      await loadSkills();
      await loadSessions();
      await loadFileTree();
      updateUI();
      elements.drawerBackdrop.classList.remove('open');
      showSuccess('Settings saved');
    });
    
    elements.btnSaveMcp.addEventListener('click', async () => {
      try {
        await invoke('mcp_save_raw', { 
          path: state.mcp?.path || null, 
          raw: elements.mcpEditor.value 
        });
        await loadMcp();
        showSuccess('MCP config saved');
      } catch (err) {
        showError('Failed to save MCP config: ' + err);
      }
    });
    
    elements.btnSaveConfig.addEventListener('click', async () => {
      try {
        await invoke('config_save_raw', { 
          path: state.config?.path || null, 
          raw: elements.configEditor.value 
        });
        await loadConfig();
        showSuccess('Config saved');
      } catch (err) {
        showError('Failed to save config: ' + err);
      }
    });
    
    // File explorer events
    if (elements.btnRefreshFiles) {
      elements.btnRefreshFiles.addEventListener('click', loadFileTree);
    }
    
    if (elements.btnCollapseExplorer) {
      elements.btnCollapseExplorer.addEventListener('click', toggleExplorer);
    }
    
    if (elements.btnToggleExplorer) {
      elements.btnToggleExplorer.addEventListener('click', toggleExplorer);
    }
    
    if (elements.btnEditFile) {
      elements.btnEditFile.addEventListener('click', enterEditMode);
    }
    
    if (elements.btnSaveFile) {
      elements.btnSaveFile.addEventListener('click', saveFile);
    }
    
    if (elements.btnCancelEdit) {
      elements.btnCancelEdit.addEventListener('click', cancelEdit);
    }
    
    // Keyboard shortcuts for file editor
    if (elements.fileEditorTextarea) {
      elements.fileEditorTextarea.addEventListener('keydown', e => {
        if ((e.metaKey || e.ctrlKey) && e.key === 's') {
          e.preventDefault();
          saveFile();
        } else if (e.key === 'Escape') {
          cancelEdit();
        }
      });
    }
    
    window.addEventListener('keydown', e => {
      if (e.key === 'Escape') {
        closeModals();
        elements.drawerBackdrop.classList.remove('open');
        closeLoginModal();
        if (elements.toolApprovalModal?.classList.contains('open')) {
          respondToolApproval(false);
        }
        hideAutocomplete();
      }
    });
    
    // Hide autocomplete when clicking outside
    document.addEventListener('click', (e) => {
      const dropdown = document.getElementById('autocomplete-dropdown');
      if (dropdown && !dropdown.contains(e.target) && 
          e.target !== elements.promptInput && e.target !== elements.chatInput) {
        hideAutocomplete();
      }
    });
  }

  init().catch((err) => {
    const message = err?.message || err || 'Initialization failed';
    showError(`Initialization failed: ${message}`);
  });
})();
