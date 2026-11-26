// Plays an HLS stream (a store trailer) in a <video>: hls.js (bundled with the
// app, loaded on first use) where the browser can't play HLS itself.
window.cerberusPlayHls = function (video, url) {
  const go = () => {
    if (window.Hls && Hls.isSupported()) {
      if (video.__hls) video.__hls.destroy();
      const h = new Hls({ capLevelToPlayerSize: true });
      h.loadSource(url);
      h.attachMedia(video);
      video.__hls = h;
      video.play().catch(() => {});
    } else {
      video.src = url;
      video.play().catch(() => {});
    }
  };
  if (window.Hls) { go(); return; }
  let s = document.querySelector('script[data-hls]');
  if (!s) {
    s = document.createElement('script');
    s.src = 'vendor/hls.light.min.js';
    s.dataset.hls = '1';
    document.head.appendChild(s);
  }
  s.addEventListener('load', go, { once: true });
};
