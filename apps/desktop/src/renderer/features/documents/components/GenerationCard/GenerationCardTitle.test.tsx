import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

import type { AiGenerationRecord } from '@ajh/shared/ipc';

import { GenerationCardTitle } from './GenerationCardTitle';

vi.mock('@ajh/translations', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const generation: AiGenerationRecord = {
  id: 'draft-document',
  createdAt: 1,
  mode: 'ats',
  board: 'linkedin',
  jobTitle: 'Analyst',
  companyName: 'Example',
  candidateName: 'Jane Doe',
  jobUrl: '',
  resumeText: 'Draft',
  coverLetterText: '',
  jobAd: '',
  companyBrief: '',
  resumeLanguage: 'en',
  jobAdLanguage: 'en',
  targetLanguage: 'en',
  mismatch: false,
  topRequirements: [],
  applicationAnswers: [],
  interviewQuestions: [],
};

describe('GenerationCardTitle application evidence', () => {
  it('labels a posting URL as a link without claiming a submission', () => {
    render(
      <GenerationCardTitle
        gen={{ ...generation, jobUrl: 'https://example.test/job' }}
        expanded={false}
        onToggle={vi.fn()}
      />
    );
    expect(screen.getByText('resumes.generated.linkedJob')).toBeInTheDocument();
    expect(screen.queryByText('resumes.generated.applied')).not.toBeInTheDocument();
  });

  it('omits the link badge when there is no posting URL', () => {
    render(<GenerationCardTitle gen={generation} expanded={false} onToggle={vi.fn()} />);
    expect(screen.queryByText('resumes.generated.linkedJob')).not.toBeInTheDocument();
    expect(screen.queryByText('resumes.generated.applied')).not.toBeInTheDocument();
  });
});
