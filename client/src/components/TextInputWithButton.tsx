import { FC } from 'react';
import { Button } from './ui/button'; // Assuming you have a Button component from ShadCN

const TextInputWithButton: FC = () => {
  return (
    <div className="flex items-center">
      {/* Input Field */}
      <input
        type="text"
        placeholder="Enter video or playlist URL"
        className="flex-1 px-4 py-2 h-full border border-black border-r-0 rounded-l-md focus:ring-2 focus:ring-blue-500"
      />
      {/* Button */}
      <Button className="px-6 py-2 h-full border border-black bg-blue-500 text-white rounded-l-none rounded-r-md hover:bg-blue-600">
        Download
      </Button>
    </div>
  );
};

export default TextInputWithButton;
