var raw = (document.documentElement && document.documentElement.dataset.base) || '';
export var BASE = raw ? (raw.endsWith('/') ? raw : raw + '/') : '';
