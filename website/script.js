gsap.registerPlugin(ScrollTrigger);

// Initialize Lenis
const lenis = new Lenis({
    lerp: 0.15, 
    smoothWheel: true,
});

lenis.on('scroll', ScrollTrigger.update);

function raf(time) {
  lenis.raf(time);
  requestAnimationFrame(raf);
}
requestAnimationFrame(raf);

// Wait for load
window.addEventListener("load", () => {
    const loader = document.querySelector('.loader');
    
    // Hide loader with GSAP
    gsap.to(loader, {
        opacity: 0,
        duration: 0.8,
        delay: 0.5,
        ease: "power2.inOut",
        onComplete: () => {
            loader.style.display = 'none';
        }
    });

    // Hero intro
    gsap.from(".reveal", {
        y: 40,
        opacity: 0,
        duration: 1.2,
        stagger: 0.15,
        ease: "power3.out",
        delay: 1.0
    });
    
    // Architecture & Feature Cards
    gsap.from(".feature-card", {
        scrollTrigger: {
            trigger: ".feature-grid",
            start: "top 85%",
        },
        y: 50,
        opacity: 0,
        duration: 0.8,
        stagger: 0.15,
        ease: "power2.out"
    });
    
    gsap.from(".arch-container", {
        scrollTrigger: {
            trigger: ".arch-container",
            start: "top 85%",
        },
        y: 50,
        opacity: 0,
        duration: 0.8,
        ease: "power2.out"
    });
    
    // Benchmark rows
    gsap.from(".b-row", {
        scrollTrigger: {
            trigger: ".b-container",
            start: "top 85%",
        },
        y: 30,
        opacity: 0,
        duration: 0.6,
        stagger: 0.15,
        ease: "power2.out"
    });
    
    // Benchmark bars expanding
    gsap.utils.toArray('.b-bar').forEach(bar => {
        gsap.to(bar, {
            scrollTrigger: {
                trigger: ".b-container",
                start: "top 85%",
            },
            width: bar.getAttribute('data-width'),
            duration: 1.5,
            delay: 0.3,
            ease: "expo.out"
        });
    });
});
