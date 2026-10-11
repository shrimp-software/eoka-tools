(() => {
    let truncated = false;
    let visited = 0;
    const deadline = performance.now() + 250;
    const excluded = 'script,style,noscript,template,input,textarea,select,option,[contenteditable]:not([contenteditable="false"])';
    function clip(value, max) {
        const bounded = String(value).slice(0, max).replace(/[\uD800-\uDBFF]$/, '');
        if (String(value).length > max) truncated = true;
        return bounded.replace(/[\u0000-\u001f\u007f-\u009f]/g, ' ').replace(/\s+/g, ' ').trim();
    }
    function visible(el) {
        for (let node = el; node; node = node.parentElement) {
            if (node.matches(excluded) || node.hidden || node.getAttribute('aria-hidden') === 'true') return false;
            const style = getComputedStyle(node);
            if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) return false;
        }
        return el.getClientRects().length > 0;
    }
    function text(root, limit) {
        let result = '';
        const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
        while (walker.nextNode()) {
            if (++visited > 10000 || performance.now() > deadline) { truncated = true; break; }
            const node = walker.currentNode;
            if (!node.parentElement || !visible(node.parentElement)) continue;
            result += ' ' + clip(node.textContent, limit - result.length + 1);
            if (result.length > limit) break;
        }
        if (visited > 10000 || performance.now() > deadline) truncated = true;
        return clip(result, limit);
    }
    function collect(selector) {
        const values = [];
        const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_ELEMENT);
        while (walker.nextNode()) {
            if (++visited > 10000 || performance.now() > deadline) { truncated = true; break; }
            const node = walker.currentNode;
            if (!node.matches(selector) || !visible(node)) continue;
            const value = text(node, 256);
            if (!value || values.includes(value)) continue;
            if (values.length === 5) { truncated = true; break; }
            values.push(value);
        }
        return values;
    }
    const headings = collect('h1,h2,h3,h4,h5,h6,[role="heading"]');
    const messages = collect('[role="alert"],[role="status"],[aria-live="polite"],[aria-live="assertive"]');
    const candidate = document.querySelector('main,[role="main"]');
    const main = candidate && visible(candidate) ? candidate : null;
    return {
        tab_id: '',
        url: clip(location.href, 4096),
        title: clip(document.title, 512),
        ready_state: document.readyState,
        headings,
        messages,
        text_excerpt: text(main || document.body || document.documentElement, 2000),
        truncated
    };
})()
