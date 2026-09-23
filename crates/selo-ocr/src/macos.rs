use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
    VNRequestTextRecognitionLevel,
};
use selo_core::{Error, Image, OcrProvider, Rect, Result, TextBlock};

pub struct MacOcr;

impl OcrProvider for MacOcr {
    fn recognize(&self, image: &Image) -> Result<Vec<TextBlock>> {
        let level = VNRequestTextRecognitionLevel::Accurate;
        let correction = true;
        let data = NSData::with_bytes(&image.png);
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &data,
            &NSDictionary::new(),
        );

        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(level);
        request.setUsesLanguageCorrection(correction);
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&[
            NSString::from_str("zh-Hans"),
            NSString::from_str("en-US"),
        ]));

        let requests: Retained<NSArray<VNRequest>> =
            NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(
                request.clone(),
            ))]);
        handler
            .performRequests_error(&requests)
            .map_err(|err| Error::Ocr(err.to_string()))?;

        let Some(results) = request.results() else {
            return Ok(Vec::new());
        };
        Ok(results
            .iter()
            .filter_map(|observation| block(&observation, image.width, image.height))
            .collect())
    }
}

fn block(observation: &VNRecognizedTextObservation, width: u32, height: u32) -> Option<TextBlock> {
    let candidate = observation.topCandidates(1).iter().next()?;
    let bb = unsafe { observation.boundingBox() };

    // Vision normalises boxes to 0..1 with the origin at the image's BOTTOM-left, y upward.
    // `TextBlock` is top-left with y downward, so the flip happens here and nowhere else.
    Some(TextBlock {
        rect: Rect {
            x: (bb.origin.x * width as f64) as f32,
            y: ((1.0 - bb.origin.y - bb.size.height) * height as f64) as f32,
            width: (bb.size.width * width as f64) as f32,
            height: (bb.size.height * height as f64) as f32,
        },
        text: candidate.string().to_string(),
        confidence: candidate.confidence(),
    })
}
