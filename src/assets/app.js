import { boot } from './modules/boot.js';
import './modules/test-hooks.js';

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', boot);
} else {
  boot();
}
