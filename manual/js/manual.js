(function(){
  var THEME_KEY = 'ccm-manual-theme';
  var themeToggle = document.getElementById('themeToggle');
  if (themeToggle){
    themeToggle.addEventListener('click', function(){
      var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
      var next = isDark ? 'light' : 'dark';
      document.documentElement.setAttribute('data-theme', next);
      themeToggle.setAttribute('aria-pressed', next === 'dark' ? 'true' : 'false');
      themeToggle.setAttribute('aria-label', next === 'dark' ? 'Switch to light theme' : 'Switch to dark theme');
      try{ localStorage.setItem(THEME_KEY, next); }catch(e){ /* storage unavailable — theme just won't persist */ }
    });
  }

  var sidebar = document.getElementById('sidebar');
  var toggle = document.getElementById('navToggle');
  var backdrop = document.getElementById('backdrop');

  function closeNav(){
    sidebar.classList.remove('is-open');
    backdrop.classList.remove('is-open');
    toggle.setAttribute('aria-expanded', 'false');
  }
  function openNav(){
    sidebar.classList.add('is-open');
    backdrop.classList.add('is-open');
    toggle.setAttribute('aria-expanded', 'true');
  }
  toggle.addEventListener('click', function(){
    sidebar.classList.contains('is-open') ? closeNav() : openNav();
  });
  backdrop.addEventListener('click', closeNav);
  document.querySelectorAll('nav.toc a').forEach(function(a){
    a.addEventListener('click', function(){ if (window.innerWidth <= 900) closeNav(); });
  });

  var filter = document.getElementById('navFilter');
  filter.addEventListener('input', function(){
    var q = filter.value.trim().toLowerCase();
    document.querySelectorAll('#tocList li').forEach(function(li){
      var text = li.textContent.toLowerCase();
      li.style.display = (!q || text.indexOf(q) !== -1) ? '' : 'none';
    });
  });

  var links = Array.prototype.slice.call(document.querySelectorAll('nav.toc a'));
  var sections = links.map(function(a){ return document.querySelector(a.getAttribute('href')); }).filter(Boolean);
  if ('IntersectionObserver' in window && sections.length){
    var byId = {};
    links.forEach(function(a){ byId[a.getAttribute('href').slice(1)] = a; });
    var observer = new IntersectionObserver(function(entries){
      entries.forEach(function(entry){
        var link = byId[entry.target.id];
        if (!link) return;
        if (entry.isIntersecting){
          links.forEach(function(l){ l.classList.remove('active'); });
          link.classList.add('active');
        }
      });
    }, { rootMargin: '-15% 0px -70% 0px', threshold: 0 });
    sections.forEach(function(s){ observer.observe(s); });
  }

  document.querySelectorAll('.copybtn').forEach(function(btn){
    btn.addEventListener('click', function(){
      var pre = btn.closest('.figure').querySelector('pre code');
      if (!pre) return;
      var text = pre.textContent;
      var done = function(){
        var original = btn.textContent;
        btn.textContent = 'Copied';
        setTimeout(function(){ btn.textContent = original; }, 1400);
      };
      try{
        navigator.clipboard.writeText(text).then(done).catch(fallback);
      }catch(e){ fallback(); }
      function fallback(){
        try{
          var ta = document.createElement('textarea');
          ta.value = text;
          ta.style.position = 'fixed';
          ta.style.opacity = '0';
          document.body.appendChild(ta);
          ta.select();
          document.execCommand('copy');
          document.body.removeChild(ta);
          done();
        }catch(e2){ /* clipboard unavailable — no-op */ }
      }
    });
  });
})();
