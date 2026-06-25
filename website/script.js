const lenis = new Lenis({
    lerp: 0.35, // Higher lerp value completely eliminates the delay feeling
    smoothWheel: true,
    wheelMultiplier: 1.5, // Faster scroll mapping
});

function raf(time) {
    lenis.raf(time);
    requestAnimationFrame(raf);
}
requestAnimationFrame(raf);

// Wait for load
window.addEventListener("load", () => {
    gsap.registerPlugin(ScrollTrigger);

    // Hide loader
    gsap.to('.loader', {
        opacity: 0,
        duration: 0.8,
        delay: 0.5,
        ease: "power2.inOut",
        onComplete: () => {
            document.querySelector('.loader').style.display = 'none';
        }
    });

    // Hero intro (delayed after loader)
    gsap.from(".reveal-text", {
        y: 40,
        opacity: 0,
        duration: 1.2,
        stagger: 0.15,
        ease: "power3.out",
        delay: 1.0
    });

    // Cards staggering
    gsap.from(".feature-card", {
        scrollTrigger: {
            trigger: ".feature-grid",
            start: "top 85%",
        },
        y: 60,
        opacity: 0,
        duration: 0.8,
        stagger: 0.15,
        ease: "power2.out"
    });

    // Benchmark bars expanding
    gsap.from(".b-bar", {
        scrollTrigger: {
            trigger: ".b-container",
            start: "top 80%",
        },
        scaleX: 0,
        duration: 1.5,
        stagger: 0.15,
        ease: "expo.out"
    });
});
