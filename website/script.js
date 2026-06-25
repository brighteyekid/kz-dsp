// Intersection Observer for scroll animations
const observerOptions = {
    root: null,
    rootMargin: '0px',
    threshold: 0.15
};

const observer = new IntersectionObserver((entries, observer) => {
    entries.forEach(entry => {
        if (entry.isIntersecting) {
            entry.target.classList.add('active');
            
            // If it's a benchmark bar row, animate the bar width
            if (entry.target.classList.contains('b-row')) {
                const bar = entry.target.querySelector('.b-bar');
                if (bar) {
                    bar.style.width = bar.getAttribute('data-width');
                }
            }
            
            observer.unobserve(entry.target);
        }
    });
}, observerOptions);

// Wait for load to remove loader and start observing
window.addEventListener("load", () => {
    const loader = document.querySelector('.loader');
    loader.style.opacity = '0';
    loader.style.transition = 'opacity 0.8s ease';
    
    setTimeout(() => {
        loader.style.display = 'none';
        
        // Start animations after loader is gone
        document.querySelectorAll('.reveal').forEach(el => {
            observer.observe(el);
        });
    }, 800);
});
